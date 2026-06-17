//! Immediate-mode 2D drawing (E8.1): a [`Frame`] lets app code emit paths,
//! shapes, and gradients with affine transforms, accumulating display-list
//! commands. The `Canvas` widget (in `lumen-widgets`) hands a `Frame` to a draw
//! closure each paint; tests render it on the deterministic CPU renderer.

use crate::display_list::{Brush, CornerRadii, DrawCmd, FillOrStroke};
use kurbo::{Affine, BezPath, Circle, Point, Rect, Shape};
use lumen_core::Color;

/// A drawing surface that accumulates [`DrawCmd`]s in node-local coordinates.
#[derive(Default)]
pub struct Frame {
    cmds: Vec<DrawCmd>,
    transform: Affine,
}

impl Frame {
    /// A new frame with an initial transform (the canvas's window-space origin).
    pub fn new(transform: Affine) -> Frame {
        Frame {
            cmds: Vec::new(),
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

    /// Consume the frame, returning its commands.
    pub fn into_cmds(self) -> Vec<DrawCmd> {
        self.cmds
    }
}
