//! Design analysis over the display list (prototype — ADR pending).
//!
//! This is the first slice of the "critique-as-data" surface proposed for the
//! agent protocol: instead of asking a vision model to eyeball a screenshot,
//! Lumen computes UI-theory metrics **deterministically from the display list**
//! and serves them as structured, node-addressable JSON.
//!
//! The metric implemented here is **text contrast** via
//! [APCA](https://github.com/Myndex/apca-w3) (the perceptual contrast model
//! being standardized for WCAG 3), with two pieces that a pixel-sampling agent
//! cannot do reliably:
//!
//! 1. [`resolve_backdrop`] composites the *actual* fill stack beneath a point
//!    (in linear light, honoring layer opacity and clips), so contrast is
//!    measured against the true rendered background — even when text sits on a
//!    translucent card over a page.
//! 2. [`apca_lc`] reports a signed lightness-contrast value (`Lc`); polarity
//!    (dark-on-light vs light-on-dark) falls out of the sign.
//!
//! Scope note: this prototype takes the text foreground + region as explicit
//! [`TextTarget`]s. In the real integration a pass extracts those from
//! `DrawCmd::GlyphRun` commands and the shaped-glyph table (T0.6); that wiring
//! is orthogonal to the algorithm de-risked here.

use crate::display_list::{Brush, DisplayList, DrawCmd};
use kurbo::{Affine, Point, Rect, Shape};
use lumen_core::Color;
use serde::Serialize;

/// A piece of text to assess: its foreground paint and the region it occupies
/// (window coordinates, logical px).
#[derive(Clone, Debug)]
pub struct TextTarget {
    /// Runtime node id (`"node-42"`), if known — lets a critique bind to the
    /// exact element so the agent can act on it.
    pub node: Option<String>,
    /// Human-readable label for reporting (e.g. the text content).
    pub label: Option<String>,
    /// The text's paint color.
    pub foreground: Color,
    /// The region the text occupies, in window coordinates.
    pub region: Rect,
}

/// APCA legibility tier for the measured `|Lc|`, following the APCA "bronze"
/// readability guidance (simplified: real thresholds also depend on font size
/// and weight, which we'll fold in once the glyph-run extraction lands).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContrastLevel {
    /// `|Lc| >= 75`: fine for body text at normal sizes.
    BodyText,
    /// `60 <= |Lc| < 75`: large or bold text only.
    LargeText,
    /// `45 <= |Lc| < 60`: headlines / non-text UI elements only.
    NonText,
    /// `|Lc| < 45`: insufficient for text.
    Fail,
}

impl ContrastLevel {
    fn from_lc(lc: f64) -> ContrastLevel {
        match lc.abs() {
            x if x >= 75.0 => ContrastLevel::BodyText,
            x if x >= 60.0 => ContrastLevel::LargeText,
            x if x >= 45.0 => ContrastLevel::NonText,
            _ => ContrastLevel::Fail,
        }
    }
}

/// `{x, y, w, h}` region, serialized to match `bounds` elsewhere in the schema.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct RegionJson {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
}

/// The contrast finding for one [`TextTarget`].
#[derive(Clone, Debug, Serialize)]
pub struct TargetContrast {
    /// Node id this finding is bound to, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// Label for reporting, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Region assessed.
    pub region: RegionJson,
    /// Foreground paint, canonical `#rrggbbaa`.
    pub foreground: String,
    /// Resolved (composited) background, canonical `#rrggbbaa`.
    pub background: String,
    /// Signed APCA lightness contrast, rounded to 1 decimal. Negative means
    /// light text on a dark background.
    pub apca_lc: f64,
    /// Legibility tier for `|apca_lc|`.
    pub level: ContrastLevel,
    /// Convenience flag: does this clear the body-text bar?
    pub passes_body_text: bool,
}

/// A full contrast report over a display list.
#[derive(Clone, Debug, Serialize)]
pub struct ContrastReport {
    /// Schema tag, versioned alongside the rest of the agent protocol.
    pub schema: &'static str,
    /// One entry per assessed target, in input order.
    pub targets: Vec<TargetContrast>,
}

/// Assess contrast for each [`TextTarget`] against its true composited
/// background. `page_bg` is the opaque window clear color (the bottom of the
/// compositing stack).
pub fn analyze_contrast(
    dl: &DisplayList,
    page_bg: Color,
    targets: &[TextTarget],
) -> ContrastReport {
    let entries = targets
        .iter()
        .map(|t| {
            let center = Point::new(
                t.region.x0 + t.region.width() / 2.0,
                t.region.y0 + t.region.height() / 2.0,
            );
            let bg = resolve_backdrop(dl, page_bg, center);
            // Flatten any translucent foreground onto the resolved backdrop so
            // the contrast reflects what the eye actually sees.
            let fg = over(t.foreground, bg);
            let lc = round1(apca_lc(fg, bg));
            TargetContrast {
                node: t.node.clone(),
                label: t.label.clone(),
                region: RegionJson {
                    x: t.region.x0,
                    y: t.region.y0,
                    w: t.region.width(),
                    h: t.region.height(),
                },
                foreground: fg.to_hex(),
                background: bg.to_hex(),
                apca_lc: lc,
                level: ContrastLevel::from_lc(lc),
                passes_body_text: ContrastLevel::from_lc(lc) == ContrastLevel::BodyText,
            }
        })
        .collect();
    ContrastReport {
        schema: "lumen-design/contrast/1",
        targets: entries,
    }
}

/// Composite the fill stack beneath `point` to recover the true background
/// color there. Walks the display list in paint order, honoring `PushLayer`
/// opacity and (axis-aligned) clip rects. Only solid fills contribute in this
/// prototype; gradients/images are skipped (documented limitation).
pub fn resolve_backdrop(dl: &DisplayList, page_bg: Color, point: Point) -> Color {
    let mut acc = page_bg; // bottom of the stack is opaque by contract
    let mut xforms: Vec<Affine> = vec![Affine::IDENTITY];
    let mut opacities: Vec<f32> = vec![1.0];
    // Current world-space clip (intersection of enclosing layer clips), if any.
    let mut clips: Vec<Option<Rect>> = vec![None];

    for cmd in &dl.cmds {
        let xform = *xforms.last().unwrap();
        let opacity = *opacities.last().unwrap();
        let clip = *clips.last().unwrap();

        match cmd {
            DrawCmd::PushLayer {
                clip: layer_clip,
                opacity: layer_op,
                transform,
                ..
            } => {
                let new_xform = xform * *transform;
                let new_clip = match layer_clip {
                    Some(rr) => {
                        let world = world_bbox(new_xform, rr.rect);
                        Some(match clip {
                            Some(c) => intersect(c, world),
                            None => world,
                        })
                    }
                    None => clip,
                };
                xforms.push(new_xform);
                opacities.push(opacity * *layer_op);
                clips.push(new_clip);
            }
            DrawCmd::PopLayer => {
                xforms.pop();
                opacities.pop();
                clips.pop();
            }
            DrawCmd::Rect {
                rect,
                brush: Brush::Solid(c),
                ..
            } => {
                let world = world_bbox(xform, *rect);
                if covers(clip, world, point) {
                    acc = over(with_layer_alpha(*c, opacity), acc);
                }
            }
            DrawCmd::Path {
                path,
                brush: Brush::Solid(c),
                style: crate::display_list::FillOrStroke::Fill,
            } => {
                let world = world_bbox(xform, path.bounding_box());
                if covers(clip, world, point) {
                    acc = over(with_layer_alpha(*c, opacity), acc);
                }
            }
            // Glyph runs, images, shaders don't establish a background here.
            _ => {}
        }
    }
    acc
}

/// Signed APCA lightness contrast `Lc` (×100) of `text` over `bg`. Both colors
/// must be opaque. Implements APCA-W3 0.1.9 ("G-4g") constants.
pub fn apca_lc(text: Color, bg: Color) -> f64 {
    const BTHRSH: f64 = 0.022; // black soft-clamp threshold
    const BCLIP: f64 = 1.414; // black soft-clamp exponent
    const DELTA_YMIN: f64 = 0.0005;
    const LO_CLIP: f64 = 0.1;
    const SCALE_BOW: f64 = 1.14; // dark-on-light
    const SCALE_WOB: f64 = 1.14; // light-on-dark
    const LO_BOW_OFFSET: f64 = 0.027;
    const LO_WOB_OFFSET: f64 = 0.027;
    const NORM_BG: f64 = 0.56;
    const NORM_TXT: f64 = 0.57;
    const REV_BG: f64 = 0.65;
    const REV_TXT: f64 = 0.62;

    let mut ytxt = apca_y(text);
    let mut ybg = apca_y(bg);
    if ytxt < BTHRSH {
        ytxt += (BTHRSH - ytxt).powf(BCLIP);
    }
    if ybg < BTHRSH {
        ybg += (BTHRSH - ybg).powf(BCLIP);
    }
    if (ybg - ytxt).abs() < DELTA_YMIN {
        return 0.0;
    }

    let out = if ybg > ytxt {
        // Normal polarity: darker text on lighter background.
        let sapc = (ybg.powf(NORM_BG) - ytxt.powf(NORM_TXT)) * SCALE_BOW;
        if sapc < LO_CLIP {
            0.0
        } else {
            sapc - LO_BOW_OFFSET
        }
    } else {
        // Reverse polarity: lighter text on darker background.
        let sapc = (ybg.powf(REV_BG) - ytxt.powf(REV_TXT)) * SCALE_WOB;
        if sapc > -LO_CLIP {
            0.0
        } else {
            sapc + LO_WOB_OFFSET
        }
    };
    out * 100.0
}

/// APCA screen luminance `Y` of an opaque color. APCA wants *simple-gamma*
/// sRGB (channel `^2.4`), so we re-encode out of Lumen's linear `Color` to sRGB
/// display values first.
fn apca_y(c: Color) -> f64 {
    let r = linear_to_srgb(c.r as f64);
    let g = linear_to_srgb(c.g as f64);
    let b = linear_to_srgb(c.b as f64);
    0.2126 * r.powf(2.4) + 0.7152 * g.powf(2.4) + 0.0722 * b.powf(2.4)
}

/// IEC 61966-2-1 sRGB OETF (linear → sRGB display value in `[0, 1]`).
fn linear_to_srgb(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Source-over compositing of `src` onto an **opaque** `dst`, in linear light.
/// Returns an opaque color (since `dst` is opaque).
fn over(src: Color, dst: Color) -> Color {
    let a = src.a.clamp(0.0, 1.0);
    Color::new_linear(
        src.r * a + dst.r * (1.0 - a),
        src.g * a + dst.g * (1.0 - a),
        src.b * a + dst.b * (1.0 - a),
        1.0,
    )
}

/// Multiply a fill's alpha by the enclosing layers' opacity product.
fn with_layer_alpha(c: Color, layer_opacity: f32) -> Color {
    Color::new_linear(c.r, c.g, c.b, c.a * layer_opacity)
}

/// World-space bounding box of `rect` under `xform` (exact for transl/scale,
/// a conservative bbox under rotation).
fn world_bbox(xform: Affine, rect: Rect) -> Rect {
    let pts = [
        xform * Point::new(rect.x0, rect.y0),
        xform * Point::new(rect.x1, rect.y0),
        xform * Point::new(rect.x1, rect.y1),
        xform * Point::new(rect.x0, rect.y1),
    ];
    let (mut minx, mut miny) = (f64::INFINITY, f64::INFINITY);
    let (mut maxx, mut maxy) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in pts {
        minx = minx.min(p.x);
        miny = miny.min(p.y);
        maxx = maxx.max(p.x);
        maxy = maxy.max(p.y);
    }
    Rect::new(minx, miny, maxx, maxy)
}

fn intersect(a: Rect, b: Rect) -> Rect {
    Rect::new(
        a.x0.max(b.x0),
        a.y0.max(b.y0),
        a.x1.min(b.x1),
        a.y1.min(b.y1),
    )
}

/// Is `point` inside `shape` and inside the current clip (if any)?
fn covers(clip: Option<Rect>, shape: Rect, point: Point) -> bool {
    if !contains(shape, point) {
        return false;
    }
    match clip {
        Some(c) => contains(c, point),
        None => true,
    }
}

fn contains(r: Rect, p: Point) -> bool {
    p.x >= r.x0 && p.x <= r.x1 && p.y >= r.y0 && p.y <= r.y1
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display_list::{BlendMode, CornerRadii};

    fn approx(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn apca_reference_values() {
        // APCA reference: black on white ≈ 106, white on black ≈ -108.
        let black = Color::BLACK;
        let white = Color::WHITE;
        assert!(
            approx(apca_lc(black, white), 106.04, 0.5),
            "black-on-white Lc was {}",
            apca_lc(black, white)
        );
        assert!(
            approx(apca_lc(white, black), -107.88, 0.5),
            "white-on-black Lc was {}",
            apca_lc(white, black)
        );
        // Polarity is encoded in the sign.
        assert!(apca_lc(black, white) > 0.0);
        assert!(apca_lc(white, black) < 0.0);
    }

    #[test]
    fn backdrop_seen_through_translucent_layer() {
        // A 50%-opaque white card over a black page: the true backdrop a glyph
        // sees is mid-gray, NOT the card's nominal white nor the page's black.
        let mut dl = DisplayList::new();
        dl.push(DrawCmd::PushLayer {
            clip: None,
            opacity: 0.5,
            transform: Affine::IDENTITY,
            blend: BlendMode::SourceOver,
        });
        dl.push(DrawCmd::Rect {
            rect: Rect::new(0.0, 0.0, 100.0, 100.0),
            brush: Brush::Solid(Color::WHITE),
            radii: CornerRadii::ZERO,
            border: None,
        });
        dl.push(DrawCmd::PopLayer);

        let bg = resolve_backdrop(&dl, Color::BLACK, Point::new(50.0, 50.0));
        // 0.5 white over black in linear light → 0.5 linear (≈ #bcbcbc sRGB).
        assert!(approx(bg.r as f64, 0.5, 1e-6), "bg.r = {}", bg.r);
        assert_eq!(bg.to_hex(), "#bcbcbcff");
    }

    #[test]
    fn clip_excludes_uncovered_point() {
        // Fill is clipped to the left half; a point on the right keeps page bg.
        let mut dl = DisplayList::new();
        dl.push(DrawCmd::PushLayer {
            clip: Some(crate::display_list::RoundedRect {
                rect: Rect::new(0.0, 0.0, 50.0, 100.0),
                radii: CornerRadii::ZERO,
            }),
            opacity: 1.0,
            transform: Affine::IDENTITY,
            blend: BlendMode::SourceOver,
        });
        dl.push(DrawCmd::Rect {
            rect: Rect::new(0.0, 0.0, 100.0, 100.0),
            brush: Brush::Solid(Color::BLACK),
            radii: CornerRadii::ZERO,
            border: None,
        });
        dl.push(DrawCmd::PopLayer);

        let inside = resolve_backdrop(&dl, Color::WHITE, Point::new(25.0, 50.0));
        let outside = resolve_backdrop(&dl, Color::WHITE, Point::new(75.0, 50.0));
        assert_eq!(inside.to_hex(), "#000000ff");
        assert_eq!(outside.to_hex(), "#ffffffff");
    }
}
