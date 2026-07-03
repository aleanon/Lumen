//! The display list: backend-independent draw commands (02 §7).
//!
//! Both the CPU (tiny-skia) and GPU (wgpu, T0.11) backends consume this same
//! list. Geometry is in window coordinates (logical px); per-command transforms
//! are expressed by enclosing [`DrawCmd::PushLayer`]/[`DrawCmd::PopLayer`] pairs.

use crate::image::RgbaImage;
use kurbo::{Affine, BezPath, Point, Rect, Shape};
use lumen_core::Color;

/// Per-corner radii for a [`DrawCmd::Rect`], in logical px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CornerRadii {
    /// Top-left radius.
    pub tl: f64,
    /// Top-right radius.
    pub tr: f64,
    /// Bottom-right radius.
    pub br: f64,
    /// Bottom-left radius.
    pub bl: f64,
}

impl CornerRadii {
    /// All corners square.
    pub const ZERO: CornerRadii = CornerRadii {
        tl: 0.0,
        tr: 0.0,
        br: 0.0,
        bl: 0.0,
    };

    /// The same radius on every corner.
    pub fn all(r: f64) -> CornerRadii {
        CornerRadii {
            tl: r,
            tr: r,
            br: r,
            bl: r,
        }
    }

    /// True if every corner is square.
    pub fn is_zero(&self) -> bool {
        self.tl == 0.0 && self.tr == 0.0 && self.br == 0.0 && self.bl == 0.0
    }
}

/// A border drawn inside a rect's edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    /// Stroke width in logical px.
    pub width: f64,
    /// Stroke color.
    pub color: Color,
}

/// Whether a path is filled or stroked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FillOrStroke {
    /// Fill the interior (non-zero winding).
    Fill,
    /// Stroke the outline with the given width.
    Stroke {
        /// Stroke width in logical px.
        width: f64,
    },
}

/// A rectangle with corner radii (used as a layer clip).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundedRect {
    /// The rectangle.
    pub rect: Rect,
    /// Corner radii.
    pub radii: CornerRadii,
}

/// Image sampling quality.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Filter {
    /// Nearest-neighbor (crisp, pixel-art).
    Nearest,
    /// Bilinear (smooth).
    Bilinear,
}

/// How a gradient behaves outside its `[0, 1]` parameter range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpreadMode {
    /// Clamp to the end colors.
    Pad,
    /// Tile.
    Repeat,
    /// Tile, mirroring every other tile.
    Reflect,
}

/// A gradient color stop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    /// Position in `[0, 1]`.
    pub offset: f32,
    /// Color at this position.
    pub color: Color,
}

/// A paint source. Gradients interpolate in Oklab (ADR-017).
#[derive(Clone, Debug, PartialEq)]
pub enum Brush {
    /// A single solid color.
    Solid(Color),
    /// An axis gradient from `start` to `end`.
    LinearGradient {
        /// Start point (offset 0).
        start: Point,
        /// End point (offset 1).
        end: Point,
        /// Color stops.
        stops: Vec<GradientStop>,
        /// Behavior outside `[0, 1]`.
        spread: SpreadMode,
    },
    /// A radial gradient centered at `center` with the given `radius`.
    RadialGradient {
        /// Center point.
        center: Point,
        /// Radius in logical px.
        radius: f64,
        /// Color stops.
        stops: Vec<GradientStop>,
        /// Behavior outside `[0, 1]`.
        spread: SpreadMode,
    },
    /// A conic (sweep) gradient around `center` starting at `start_angle` (rad).
    ConicGradient {
        /// Center point.
        center: Point,
        /// Starting angle in radians (0 = +x axis).
        start_angle: f64,
        /// Color stops, swept through a full turn.
        stops: Vec<GradientStop>,
    },
}

/// Compositing blend mode for a layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlendMode {
    /// Normal source-over.
    SourceOver,
    /// Multiply.
    Multiply,
    /// Screen.
    Screen,
    /// Overlay.
    Overlay,
    /// Darken.
    Darken,
    /// Lighten.
    Lighten,
}

/// Index into the display list's image table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageId(pub u32);

/// Index into [`DisplayList::runs`], the shaped-glyph-run table (R3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphRunId(pub u32);

/// A rasterized glyph's 8-bit coverage bitmap. The text stack rasterizes glyphs
/// (it owns the font/shaper; the renderer does not) and hands them across the
/// boundary in [`DisplayList::glyph_images`]. `key` is a stable identity (font +
/// glyph id + physical size + subpixel bucket + embolden) so a GPU atlas can
/// cache and evict entries across frames (R3.2b).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlyphImage {
    /// Stable cross-frame identity for atlas caching.
    pub key: u64,
    /// Bitmap width in px.
    pub width: u32,
    /// Bitmap height in px.
    pub height: u32,
    /// Row-major 8-bit coverage, `width * height` bytes.
    pub coverage: Vec<u8>,
}

/// One glyph placed in window space, referencing a [`GlyphImage`] by index into
/// [`DisplayList::glyph_images`]. The destination rect is in **logical** px while
/// the coverage bitmap may be rasterized at a higher physical resolution (R3.5);
/// the renderer scales the logical rect to physical and samples the bitmap 1:1,
/// keeping HiDPI text crisp. At scale 1 the rect size equals the bitmap size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    /// Index into [`DisplayList::glyph_images`].
    pub image: u32,
    /// Left of the destination rect, in window (logical) px.
    pub x: f32,
    /// Top of the destination rect, in window (logical) px.
    pub y: f32,
    /// Destination width in logical px (`bitmap_width / scale`).
    pub w: f32,
    /// Destination height in logical px (`bitmap_height / scale`).
    pub h: f32,
}

/// A shaped run of glyphs sharing a paint color (set by the run's
/// [`DrawCmd::GlyphRun`]). Indexed by [`GlyphRunId`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlyphRun {
    /// Placed glyphs, in paint order.
    pub glyphs: Vec<PlacedGlyph>,
}

/// Index into the shader table (T4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShaderId(pub u32);

/// Shader uniforms. On the CPU backend a shader renders a deterministic
/// `fallback` fill (02 §7); the real pipeline runs on the GPU backend.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UniformBlock {
    /// The deterministic fill used by the CPU fallback.
    pub fallback: Color,
}

impl Default for UniformBlock {
    fn default() -> Self {
        UniformBlock {
            fallback: Color::srgb8(128, 128, 128, 255),
        }
    }
}

/// A single draw command (02 §7).
#[derive(Clone, Debug, PartialEq)]
pub enum DrawCmd {
    /// A (possibly rounded) rectangle, filled with `brush`, optionally bordered.
    Rect {
        /// The rectangle.
        rect: Rect,
        /// Fill paint.
        brush: Brush,
        /// Corner radii.
        radii: CornerRadii,
        /// Optional border stroke.
        border: Option<Border>,
    },
    /// An arbitrary path, filled or stroked with `brush`.
    Path {
        /// The path geometry.
        path: BezPath,
        /// Paint.
        brush: Brush,
        /// Fill or stroke.
        style: FillOrStroke,
    },
    /// Draw a region of an image into `dst_rect`.
    Image {
        /// Image table index.
        id: ImageId,
        /// Source region within the image.
        src_rect: Rect,
        /// Destination rect in window coords.
        dst_rect: Rect,
        /// Sampling quality.
        quality: Filter,
    },
    /// A shaped glyph run, painted with `brush`. Rendered by the text stack
    /// (T0.6); a no-op on the CPU backend until then.
    GlyphRun {
        /// Glyph-run table index ([`DisplayList::runs`]).
        run: GlyphRunId,
        /// Text paint (the whole run shares one color).
        brush: Brush,
        /// Bounding rect (window px) covering the run's glyphs — for damage
        /// tracking, since the glyph positions live in the run table.
        rect: Rect,
    },
    /// Begin a layer; subsequent commands draw into it until [`DrawCmd::PopLayer`].
    PushLayer {
        /// Optional rounded-rect clip applied when the layer composites.
        clip: Option<RoundedRect>,
        /// Layer opacity in `[0, 1]`.
        opacity: f32,
        /// Transform applied when the layer composites onto its parent.
        transform: Affine,
        /// Blend mode used when compositing.
        blend: BlendMode,
    },
    /// End the current layer and composite it onto its parent.
    PopLayer,
    /// A custom shader fill over `rect` (deterministic fallback on CPU).
    Shader {
        /// Shader table index.
        id: ShaderId,
        /// Region to fill.
        rect: Rect,
        /// Uniforms (carry the CPU fallback color).
        uniforms: UniformBlock,
    },
    /// Blur (and optionally saturate) the already-painted backdrop within a
    /// rounded-rect region — the glass `backdrop-filter`. Emitted *before* the
    /// node's translucent fill, so it filters everything painted behind it.
    BackdropFilter {
        /// Region to filter, in window coordinates.
        rect: Rect,
        /// Rounded-corner clip for the region.
        radii: CornerRadii,
        /// Blur radius in logical px (`0` = none).
        blur: f32,
        /// Saturation multiplier applied to the blurred backdrop (`1.0` = none).
        saturate: f32,
        /// Edge refraction/lensing strength in logical px (`0` = none): bends the
        /// blurred backdrop along the rounded-rect edge normal, strongest at the
        /// edge (Liquid Glass).
        refraction: f32,
        /// Specular edge-highlight intensity (`0` = none): a bright rim along the
        /// top-left-lit edge.
        specular: f32,
    },
}

/// A complete display list plus the resources its commands reference.
#[derive(Clone, Debug, Default)]
pub struct DisplayList {
    /// The draw commands, in paint order.
    pub cmds: Vec<DrawCmd>,
    /// Images referenced by [`DrawCmd::Image`] via [`ImageId`].
    pub images: Vec<RgbaImage>,
    /// Shaped glyph runs referenced by [`DrawCmd::GlyphRun`] via [`GlyphRunId`].
    pub runs: Vec<GlyphRun>,
    /// Deduplicated glyph coverage bitmaps referenced by [`PlacedGlyph::image`].
    pub glyph_images: Vec<GlyphImage>,
}

impl DisplayList {
    /// An empty display list.
    pub fn new() -> DisplayList {
        DisplayList::default()
    }

    /// Append a command.
    pub fn push(&mut self, cmd: DrawCmd) {
        self.cmds.push(cmd);
    }

    /// Add a glyph run, returning its [`GlyphRunId`].
    pub fn add_run(&mut self, run: GlyphRun) -> GlyphRunId {
        let id = GlyphRunId(self.runs.len() as u32);
        self.runs.push(run);
        id
    }

    /// Intern a glyph image, deduplicating by its stable `key`, and return its
    /// index for [`PlacedGlyph::image`]. The same glyph used many times across
    /// runs is stored once.
    pub fn intern_glyph(&mut self, img: GlyphImage) -> u32 {
        if let Some(i) = self.glyph_images.iter().position(|g| g.key == img.key) {
            return i as u32;
        }
        let i = self.glyph_images.len() as u32;
        self.glyph_images.push(img);
        i
    }

    /// Intern by reference (R5): look the glyph up by stable key and **clone** the
    /// coverage image only when it isn't already present — so reusing a cached run
    /// pays a clone only for glyphs new to this frame.
    pub fn intern_glyph_ref(&mut self, img: &GlyphImage) -> u32 {
        if let Some(i) = self.glyph_images.iter().position(|g| g.key == img.key) {
            return i as u32;
        }
        let i = self.glyph_images.len() as u32;
        self.glyph_images.push(img.clone());
        i
    }
}

impl DrawCmd {
    /// The screen-space rectangle (logical px) this command paints into, or
    /// `None` for structural/unbounded commands (`PushLayer`/`PopLayer`,
    /// `GlyphRun`) — a `None` in a changed range forces a full repaint. AA seams
    /// and centered borders/strokes are accounted for by a small inflation.
    pub fn paint_bounds(&self) -> Option<Rect> {
        match self {
            DrawCmd::Rect { rect, border, .. } => {
                let g = border.map(|b| b.width / 2.0).unwrap_or(0.0) + 1.0;
                Some(rect.inflate(g, g))
            }
            DrawCmd::Path { path, style, .. } => {
                let g = match style {
                    FillOrStroke::Fill => 1.0,
                    FillOrStroke::Stroke { width } => width / 2.0 + 1.0,
                };
                Some(path.bounding_box().inflate(g, g))
            }
            DrawCmd::Image { dst_rect, .. } => Some(*dst_rect),
            DrawCmd::Shader { rect, .. } => Some(*rect),
            DrawCmd::BackdropFilter {
                rect,
                blur,
                refraction,
                ..
            } => {
                let g = (*blur + *refraction) as f64 + 1.0;
                Some(rect.inflate(g, g))
            }
            DrawCmd::GlyphRun { rect, .. } => Some(rect.inflate(1.0, 1.0)),
            DrawCmd::PushLayer { .. } | DrawCmd::PopLayer => None,
        }
    }
}

/// The region of a frame that changed between two display lists (R2.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Damage {
    /// Nothing changed — the previous frame can be reused verbatim.
    None,
    /// Only this (logical-px) rectangle changed; repaint just it.
    Region(Rect),
    /// The change can't be bounded to a sub-region; repaint the whole frame.
    Full,
}

/// Two `DrawCmd`s are equal *and* (for images) reference byte-identical pixels.
/// The derived `==` only compares the image *index*, so cached text/shadow
/// sprites that change content at a stable list position need the byte check.
fn cmd_eq(a: &DrawCmd, b: &DrawCmd, prev: &DisplayList, next: &DisplayList) -> bool {
    // The `DrawCmd` itself (incl. an Image/GlyphRun table index) must match...
    if a != b {
        return false;
    }
    // ...and so must the *content* it references by index, since the index is
    // stable across frames but the table behind it isn't.
    match (a, b) {
        (DrawCmd::Image { id: ia, .. }, DrawCmd::Image { id: ib, .. }) => {
            prev.images.get(ia.0 as usize).map(|i| i.pixels())
                == next.images.get(ib.0 as usize).map(|i| i.pixels())
        }
        (DrawCmd::GlyphRun { run: ra, .. }, DrawCmd::GlyphRun { run: rb, .. }) => {
            run_content_eq(prev, *ra, next, *rb)
        }
        _ => true,
    }
}

/// Whether two glyph runs paint the same thing: same glyphs at the same
/// positions with the same bitmaps (compared by stable glyph key).
fn run_content_eq(prev: &DisplayList, ra: GlyphRunId, next: &DisplayList, rb: GlyphRunId) -> bool {
    let (Some(pr), Some(nr)) = (prev.runs.get(ra.0 as usize), next.runs.get(rb.0 as usize)) else {
        return false;
    };
    pr.glyphs.len() == nr.glyphs.len()
        && pr.glyphs.iter().zip(&nr.glyphs).all(|(p, n)| {
            p.x == n.x
                && p.y == n.y
                && prev.glyph_images.get(p.image as usize).map(|g| g.key)
                    == next.glyph_images.get(n.image as usize).map(|g| g.key)
        })
}

/// Compute the [`Damage`] between a previous display list and the next one.
///
/// Trims the common (content-equal) prefix and suffix; the difference in the
/// middle is the change. The damage rect is the union of the changed commands'
/// [`paint_bounds`](DrawCmd::paint_bounds) from *both* lists (so a region a
/// command vacated is repainted too). Any unbounded changed command ⇒ `Full`.
pub fn damage_between(prev: &DisplayList, next: &DisplayList) -> Damage {
    let (po, pn) = (&prev.cmds, &next.cmds);
    let max_p = po.len().min(pn.len());
    let mut p = 0;
    while p < max_p && cmd_eq(&po[p], &pn[p], prev, next) {
        p += 1;
    }
    let mut s = 0;
    while s < (po.len() - p).min(pn.len() - p)
        && cmd_eq(&po[po.len() - 1 - s], &pn[pn.len() - 1 - s], prev, next)
    {
        s += 1;
    }
    let changed_old = &po[p..po.len() - s];
    let changed_new = &pn[p..pn.len() - s];
    if changed_old.is_empty() && changed_new.is_empty() {
        return Damage::None;
    }
    let mut rect: Option<Rect> = None;
    for c in changed_old.iter().chain(changed_new.iter()) {
        match c.paint_bounds() {
            Some(b) => rect = Some(rect.map_or(b, |r: Rect| r.union(b))),
            None => return Damage::Full,
        }
    }
    match rect {
        Some(r) => Damage::Region(r),
        None => Damage::None,
    }
}
