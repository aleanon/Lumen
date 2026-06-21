//! Immediate-mode 2D drawing (E8.1): a [`Frame`] lets app code emit paths,
//! shapes, and gradients with affine transforms, accumulating display-list
//! commands. The `Canvas` widget (in `lumen-widgets`) hands a `Frame` to a draw
//! closure each paint; tests render it on the deterministic CPU renderer.

use crate::display_list::{Brush, CornerRadii, DrawCmd, FillOrStroke};
use kurbo::{Affine, BezPath, Circle, Point, Rect, Shape};
use lumen_core::Color;

/// Horizontal anchor for [`Frame::fill_text`] — which part of the text box sits
/// at the given point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnchorX {
    /// The point is the text's left edge.
    #[default]
    Start,
    /// The point is the text's horizontal centre.
    Center,
    /// The point is the text's right edge.
    End,
}

/// Vertical anchor for [`Frame::fill_text`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnchorY {
    /// The point is the text's top edge.
    #[default]
    Top,
    /// The point is the text's vertical middle.
    Middle,
    /// The point is the text's bottom edge.
    Bottom,
}

/// Text styling for [`Frame::fill_text`]. A plain struct (lumen-render can't
/// depend on lumen-text), resolved into shaped glyphs by the widget runtime.
#[derive(Clone, Copy, Debug)]
pub struct TextOpts {
    /// Font size in logical px.
    pub size: f32,
    /// Font weight (400 = regular, 700 = bold).
    pub weight: f32,
    /// Fill colour.
    pub color: Color,
    /// Horizontal anchor.
    pub anchor_x: AnchorX,
    /// Vertical anchor.
    pub anchor_y: AnchorY,
}

impl Default for TextOpts {
    fn default() -> TextOpts {
        TextOpts {
            size: 14.0,
            weight: 400.0,
            color: Color::BLACK,
            anchor_x: AnchorX::Start,
            anchor_y: AnchorY::Top,
        }
    }
}

/// A deferred text-draw intent recorded by [`Frame::fill_text`]. The `Frame`
/// can't shape text itself (no `TextEngine` here), so the widget runtime
/// rasterizes these against its text stack after the draw closure runs.
#[derive(Clone, Debug)]
pub struct FrameText {
    /// Anchor point in window coordinates (transform already applied).
    pub pos: Point,
    /// The string to draw (single line).
    pub text: String,
    /// Styling + anchoring.
    pub opts: TextOpts,
}

/// A drawing surface that accumulates [`DrawCmd`]s in node-local coordinates.
#[derive(Default)]
pub struct Frame {
    cmds: Vec<DrawCmd>,
    texts: Vec<FrameText>,
    transform: Affine,
}

impl Frame {
    /// A new frame with an initial transform (the canvas's window-space origin).
    pub fn new(transform: Affine) -> Frame {
        Frame {
            cmds: Vec::new(),
            texts: Vec::new(),
            transform,
        }
    }

    /// Fill `path` with a solid color.
    pub fn fill(&mut self, path: &BezPath, color: Color) {
        let mut p = path.clone();
        p.apply_affine(self.transform);
        self.cmds.push(DrawCmd::Path {
            path: p,
            brush: Brush::Solid(color),
            style: FillOrStroke::Fill,
        });
    }

    /// Stroke `path` with a solid color and width.
    pub fn stroke(&mut self, path: &BezPath, color: Color, width: f64) {
        let mut p = path.clone();
        p.apply_affine(self.transform);
        self.cmds.push(DrawCmd::Path {
            path: p,
            brush: Brush::Solid(color),
            style: FillOrStroke::Stroke { width },
        });
    }

    /// Fill an axis-aligned rectangle with a brush (transform must be
    /// translate/scale; use [`Frame::fill`] for rotated rects).
    pub fn fill_rect(&mut self, rect: Rect, brush: Brush) {
        let a = self.transform * Point::new(rect.x0, rect.y0);
        let b = self.transform * Point::new(rect.x1, rect.y1);
        self.cmds.push(DrawCmd::Rect {
            rect: Rect::from_points(a, b),
            brush,
            radii: CornerRadii::all(0.0),
            border: None,
        });
    }

    /// Fill a circle with a solid color.
    pub fn fill_circle(&mut self, center: Point, radius: f64, color: Color) {
        self.fill(&Circle::new(center, radius).to_path(0.1), color);
    }

    /// Fill a rounded rectangle with a solid color.
    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: f64, color: Color) {
        self.fill(
            &kurbo::RoundedRect::from_rect(rect, radius).to_path(0.1),
            color,
        );
    }

    /// Run `draw` with `transform` composed onto the current one (e.g. a
    /// rotation about a pivot), restoring afterward.
    pub fn with_transform(&mut self, transform: Affine, draw: impl FnOnce(&mut Frame)) {
        let prev = self.transform;
        self.transform = prev * transform;
        draw(self);
        self.transform = prev;
    }

    /// Fill a rectangle with a horizontal two-color linear gradient.
    pub fn linear_gradient_rect(&mut self, rect: Rect, a: Color, b: Color) {
        use crate::display_list::{GradientStop, SpreadMode};
        let p0 = self.transform * Point::new(rect.x0, rect.y0);
        let p1 = self.transform * Point::new(rect.x1, rect.y1);
        let r = Rect::from_points(p0, p1);
        self.cmds.push(DrawCmd::Rect {
            rect: r,
            brush: Brush::LinearGradient {
                start: Point::new(r.x0, r.y0),
                end: Point::new(r.x1, r.y0),
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        color: a,
                    },
                    GradientStop {
                        offset: 1.0,
                        color: b,
                    },
                ],
                spread: SpreadMode::Pad,
            },
            radii: CornerRadii::all(0.0),
            border: None,
        });
    }

    /// Draw a single line of text with its `opts.anchor_*` point at `pos`. The
    /// frame can't shape glyphs itself, so this records a [`FrameText`] intent
    /// that the widget runtime rasterizes against its text stack (`into_parts`).
    /// The transform's translation positions the text; font size is not scaled
    /// (use it for translate-only transforms, like axis/label placement).
    pub fn fill_text(&mut self, pos: Point, text: impl Into<String>, opts: TextOpts) {
        let p = self.transform * pos;
        self.texts.push(FrameText {
            pos: p,
            text: text.into(),
            opts,
        });
    }

    /// Consume the frame, returning its commands.
    pub fn into_cmds(self) -> Vec<DrawCmd> {
        self.cmds
    }

    /// Consume the frame, returning its draw commands and deferred text intents.
    pub fn into_parts(self) -> (Vec<DrawCmd>, Vec<FrameText>) {
        (self.cmds, self.texts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_text_records_intent_with_transform_applied() {
        // The frame's origin transform offsets the anchor point, but draw
        // commands and text intents are separate channels (into_parts).
        let mut f = Frame::new(Affine::translate((100.0, 50.0)));
        f.fill_text(
            Point::new(10.0, 8.0),
            "42",
            TextOpts {
                size: 12.0,
                anchor_x: AnchorX::Center,
                ..TextOpts::default()
            },
        );
        let (cmds, texts) = f.into_parts();
        assert!(cmds.is_empty(), "fill_text emits no draw cmd directly");
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0].text, "42");
        assert_eq!(texts[0].pos, Point::new(110.0, 58.0));
        assert_eq!(texts[0].opts.anchor_x, AnchorX::Center);
    }
}
