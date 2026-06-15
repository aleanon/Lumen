//! `lumen-text` — text shaping, layout, measurement, and CPU rasterization.
//!
//! parley does shaping + layout (bidi, line breaking, alignment); swash does
//! glyph scaling/rasterization (ADR-005). Only the bundled pan-Unicode Noto
//! font is used — never system fonts — so shaping and rasterization are
//! deterministic across machines, which is what makes text goldens trustworthy.
#![warn(missing_docs)]

use kurbo::Size;
use lumen_core::Color;
use lumen_render::RgbaImage;
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontStack, Layout, LayoutContext,
    PositionedLayoutItem, StyleProperty,
};
use std::borrow::Cow;
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::Format;
use swash::FontRef;

/// Bundled pan-Unicode Noto font (Latin/CJK/Arabic/Hebrew). No system fonts
/// (ADR-005). Color emoji is out of M0 scope; see the decision log.
const FONT: &[u8] = include_bytes!("../fonts/GoNotoKurrent-Regular.ttf");

pub mod editor;
pub use editor::{Preedit, TextEditor};

/// Brush carried through parley to each glyph run: straight sRGB RGBA8.
type Brush = [u8; 4];

/// Horizontal alignment of wrapped lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextAlign {
    /// Align to the start (left for LTR).
    #[default]
    Start,
    /// Center.
    Center,
    /// Align to the end.
    End,
}

/// A run of text styling.
#[derive(Clone, Copy, Debug)]
pub struct TextStyle {
    /// Font size in logical px.
    pub font_size: f32,
    /// Text color.
    pub color: Color,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            font_size: 16.0,
            color: Color::BLACK,
        }
    }
}

/// The text engine: owns the bundled-font context. Reuse across layouts.
pub struct TextEngine {
    font_cx: FontContext,
    layout_cx: LayoutContext<Brush>,
    family: String,
}

impl Default for TextEngine {
    fn default() -> Self {
        TextEngine::new()
    }
}

impl TextEngine {
    /// Build an engine with only the bundled font registered (no system fonts).
    pub fn new() -> TextEngine {
        let mut collection =
            parley::fontique::Collection::new(parley::fontique::CollectionOptions {
                shared: false,
                system_fonts: false,
            });
        let registered = collection.register_fonts(FONT.to_vec());
        let family = registered
            .first()
            .and_then(|(id, _)| collection.family_name(*id))
            .unwrap_or("Noto")
            .to_string();
        TextEngine {
            font_cx: FontContext {
                collection,
                source_cache: parley::fontique::SourceCache::default(),
            },
            layout_cx: LayoutContext::new(),
            family,
        }
    }

    /// Shape and lay out `text`. `ranges` apply per-byte-range style overrides
    /// (multi-style runs). `max_width` enables wrapping (UAX #14); `None` = no
    /// wrap. Returns a measured, renderable block.
    pub fn layout(
        &mut self,
        text: &str,
        base: TextStyle,
        ranges: &[(std::ops::Range<usize>, TextStyle)],
        max_width: Option<f32>,
        align: TextAlign,
    ) -> TextBlock {
        let mut builder = self.layout_cx.ranged_builder(&mut self.font_cx, text, 1.0);
        builder.push_default(StyleProperty::FontStack(FontStack::Single(
            FontFamily::Named(Cow::Owned(self.family.clone())),
        )));
        builder.push_default(StyleProperty::FontSize(base.font_size));
        builder.push_default(StyleProperty::Brush(base.color.to_srgb8()));
        for (range, style) in ranges {
            builder.push(StyleProperty::FontSize(style.font_size), range.clone());
            builder.push(StyleProperty::Brush(style.color.to_srgb8()), range.clone());
        }
        let mut layout: Layout<Brush> = builder.build(text);
        layout.break_all_lines(max_width);
        let parley_align = match align {
            TextAlign::Start => Alignment::Start,
            TextAlign::Center => Alignment::Middle,
            TextAlign::End => Alignment::End,
        };
        layout.align(max_width, parley_align, AlignmentOptions::default());
        TextBlock { layout }
    }

    /// The x-position (logical px) of byte offset `byte` in `text` at `base`
    /// style, measured by laying out the prefix. Used for selection/caret
    /// geometry (T1.5). `byte` must be a char boundary.
    pub fn measure_prefix(&mut self, text: &str, base: TextStyle, byte: usize) -> f32 {
        if byte == 0 {
            return 0.0;
        }
        self.layout(&text[..byte], base, &[], None, TextAlign::Start)
            .width()
    }

    /// Lay out `text` on a single line, truncating with an ellipsis (`…`) if it
    /// exceeds `max_width` (text-overflow: ellipsis).
    pub fn layout_ellipsized(&mut self, text: &str, base: TextStyle, max_width: f32) -> TextBlock {
        let full = self.layout(text, base, &[], None, TextAlign::Start);
        if full.width() <= max_width {
            return full;
        }
        let ellipsis = '…';
        let mut best = String::from(ellipsis);
        let mut acc = String::new();
        for ch in text.chars() {
            acc.push(ch);
            let candidate = format!("{acc}{ellipsis}");
            if self
                .layout(&candidate, base, &[], None, TextAlign::Start)
                .width()
                <= max_width
            {
                best = candidate;
            } else {
                break;
            }
        }
        self.layout(&best, base, &[], None, TextAlign::Start)
    }
}

/// A laid-out, measured block of text, renderable to an [`RgbaImage`].
pub struct TextBlock {
    layout: Layout<Brush>,
}

impl TextBlock {
    /// The measured width in logical px (stable across runs).
    pub fn width(&self) -> f32 {
        self.layout.width()
    }

    /// The measured height in logical px.
    pub fn height(&self) -> f32 {
        self.layout.height()
    }

    /// The measured size.
    pub fn size(&self) -> Size {
        Size::new(self.width() as f64, self.height() as f64)
    }

    /// Rasterize onto a `width`×`height` image over `background` (CPU path).
    /// `width`/`height` default to the measured size if zero.
    pub fn render(&self, width: u32, height: u32, background: Color) -> RgbaImage {
        self.render_inner(width, height, background, None)
    }

    /// Like [`TextBlock::render`], but paints a selection highlight from `x0` to
    /// `x1` (logical px) behind the text (T1.5 selection rendering).
    pub fn render_with_selection(
        &self,
        width: u32,
        height: u32,
        background: Color,
        x0: f32,
        x1: f32,
        highlight: Color,
    ) -> RgbaImage {
        self.render_inner(width, height, background, Some((x0, x1, highlight)))
    }

    fn render_inner(
        &self,
        width: u32,
        height: u32,
        background: Color,
        selection: Option<(f32, f32, Color)>,
    ) -> RgbaImage {
        let w = if width == 0 {
            self.width().ceil() as u32
        } else {
            width
        }
        .max(1);
        let h = if height == 0 {
            self.height().ceil() as u32
        } else {
            height
        }
        .max(1);
        let bg = background.to_srgb8();
        let mut pixels = vec![0u8; (w as usize) * (h as usize) * 4];
        for px in pixels.chunks_exact_mut(4) {
            px.copy_from_slice(&bg);
        }

        // Selection highlight (opaque fill) behind the glyphs.
        if let Some((sx0, sx1, color)) = selection {
            let hc = color.to_srgb8();
            let cx0 = sx0.max(0.0) as u32;
            let cx1 = (sx1.max(0.0) as u32).min(w);
            for y in 0..h {
                for x in cx0..cx1 {
                    let idx = ((y * w + x) * 4) as usize;
                    pixels[idx..idx + 4].copy_from_slice(&hc);
                }
            }
        }

        let mut ctx = ScaleContext::new();
        for line in self.layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let color = glyph_run.style().brush;
                let font = run.font();
                let Some(font_ref) = FontRef::from_index(font.data.as_ref(), font.index as usize)
                else {
                    continue;
                };
                let mut scaler = ctx
                    .builder(font_ref)
                    .size(run.font_size())
                    .hint(true)
                    .normalized_coords(run.normalized_coords())
                    .build();
                for glyph in glyph_run.positioned_glyphs() {
                    let Some(image) = Render::new(&[Source::Outline])
                        .format(Format::Alpha)
                        .render(&mut scaler, glyph.id)
                    else {
                        continue;
                    };
                    blit_alpha(&mut pixels, w, h, &image, glyph.x, glyph.y, color);
                }
            }
        }
        RgbaImage::from_raw(w, h, pixels)
    }
}

/// Composite a swash alpha glyph image onto the target at the glyph pen
/// position, in straight-alpha sRGB.
fn blit_alpha(
    pixels: &mut [u8],
    w: u32,
    h: u32,
    image: &swash::scale::image::Image,
    pen_x: f32,
    pen_y: f32,
    color: Brush,
) {
    let gx = pen_x.round() as i32 + image.placement.left;
    let gy = pen_y.round() as i32 - image.placement.top;
    let gw = image.placement.width as i32;
    let gh = image.placement.height as i32;
    for row in 0..gh {
        let py = gy + row;
        if py < 0 || py >= h as i32 {
            continue;
        }
        for col in 0..gw {
            let pxc = gx + col;
            if pxc < 0 || pxc >= w as i32 {
                continue;
            }
            let a = image.data[(row * gw + col) as usize] as f32 / 255.0;
            if a <= 0.0 {
                continue;
            }
            let src_a = a * (color[3] as f32 / 255.0);
            let idx = ((py as u32 * w + pxc as u32) * 4) as usize;
            for c in 0..3 {
                let src = color[c] as f32;
                let dst = pixels[idx + c] as f32;
                pixels[idx + c] = (src * src_a + dst * (1.0 - src_a)).round() as u8;
            }
            let dst_a = pixels[idx + 3] as f32 / 255.0;
            pixels[idx + 3] = ((src_a + dst_a * (1.0 - src_a)) * 255.0).round() as u8;
        }
    }
}
