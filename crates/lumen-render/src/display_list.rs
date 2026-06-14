//! The display list: backend-independent draw commands (02 §7).
//!
//! Both the CPU (tiny-skia) and GPU (wgpu, T0.11) backends consume this same
//! list. Geometry is in window coordinates (logical px); per-command transforms
//! are expressed by enclosing [`DrawCmd::PushLayer`]/[`DrawCmd::PopLayer`] pairs.

use crate::image::RgbaImage;
use kurbo::{Affine, BezPath, Point, Rect};
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
#[derive(Clone, Copy, Debug)]
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
#[derive(Clone, Copy, Debug)]
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
#[derive(Clone, Copy, Debug)]
pub struct GradientStop {
    /// Position in `[0, 1]`.
    pub offset: f32,
    /// Color at this position.
    pub color: Color,
}

/// A paint source. Gradients interpolate in Oklab (ADR-017).
#[derive(Clone, Debug)]
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

/// Index into the shaped-glyph-run table (populated by the text stack, T0.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphRunId(pub u32);

/// Index into the shader table (T4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShaderId(pub u32);

/// Shader uniforms. On the CPU backend a shader renders a deterministic
/// `fallback` fill (02 §7); the real pipeline runs on the GPU backend.
#[derive(Clone, Copy, Debug)]
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
#[derive(Clone, Debug)]
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
        /// Glyph-run table index.
        run: GlyphRunId,
        /// Text paint.
        brush: Brush,
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
}

/// A complete display list plus the resources its commands reference.
#[derive(Clone, Debug, Default)]
pub struct DisplayList {
    /// The draw commands, in paint order.
    pub cmds: Vec<DrawCmd>,
    /// Images referenced by [`DrawCmd::Image`] via [`ImageId`].
    pub images: Vec<RgbaImage>,
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
}
