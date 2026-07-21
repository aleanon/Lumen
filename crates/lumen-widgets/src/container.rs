//! [`Container`] — a flex layout box. Its `Element` is built inside
//! [`Container::new`]; modifiers set direction, spacing, padding, alignment, and
//! size.

use crate::widget::impl_common;
use crate::Element;
use lumen_core::semantics::Role;
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};

/// A flex container (column by default) holding child elements. Use it to group,
/// pad, space, align, and size a subtree.
/// # Example
///
/// ```
/// use lumen_widgets::{widgets, App, Container};
///
/// let app = App::new(|_| {
///     Container::new(vec![widgets::text("A"), widgets::text("B")]).row().gap(8.0).padding(6.0).into()
/// });
/// # lumen_widgets::doc_shot(app, 120.0, 48.0, "container");
/// ```
///
/// Renders:
///
/// ![Container example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/container.png)
///
/// The picture above is `src/doc_shots/container.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Container {
    el: Element,
}

impl Container {
    /// A column container wrapping `children`.
    pub fn new(children: impl Into<Vec<Element>>) -> Container {
        let el = Element {
            role: Role::Group,
            elide_semantics: true,
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..LayoutStyle::default()
            },
            children: children.into(),
            ..Element::default()
        };
        Container { el }
    }

    /// Lay children out in a row instead of a column.
    pub fn row(mut self) -> Container {
        self.el.style.flex_direction = FlexDirection::Row;
        self
    }

    /// Lay children out in a column (the default).
    pub fn column(mut self) -> Container {
        self.el.style.flex_direction = FlexDirection::Column;
        self
    }

    /// Uniform padding on all sides (px).
    /// Overlay layout: children stack at the top-left, last on top (the
    /// typed form of `widgets::stack`).
    pub fn stack(mut self) -> Container {
        use lumen_layout::{Edges, Position};
        self.el.style.position = Position::Relative;
        self.el.style.display = lumen_layout::Display::Flex;
        for c in &mut self.el.children {
            c.style.position = Position::Absolute;
            c.style.inset = Edges {
                left: Dim::px(0.0),
                top: Dim::px(0.0),
                ..Edges::AUTO
            };
        }
        self.el.elide_semantics = true;
        self
    }

    /// Uniform padding (px) on all sides.
    pub fn padding(mut self, px: f32) -> Container {
        self.el.style.padding = Edges::all(Dim::px(px));
        self
    }

    /// Gap between children on both axes (px).
    pub fn gap(mut self, px: f32) -> Container {
        self.el.style.row_gap = Dim::px(px);
        self.el.style.column_gap = Dim::px(px);
        self
    }

    /// Cross-axis alignment of children (`align-items`).
    pub fn align(mut self, a: Align) -> Container {
        self.el.style.align_items = Some(a);
        self
    }

    /// Main-axis distribution of children (`justify-content`).
    pub fn justify(mut self, a: Align) -> Container {
        self.el.style.justify_content = Some(a);
        self
    }

    /// Fixed width in px.
    pub fn width(mut self, px: f32) -> Container {
        self.el.style.width = Dim::px(px);
        self
    }

    /// Fixed height in px.
    pub fn height(mut self, px: f32) -> Container {
        self.el.style.height = Dim::px(px);
        self
    }

    /// Fill the parent on both axes.
    pub fn fill(mut self) -> Container {
        self.el.style.width = Dim::pct(1.0);
        self.el.style.height = Dim::pct(1.0);
        self
    }

    /// Rounded corners (px).
    pub fn corner_radius(mut self, px: f64) -> Container {
        self.el.corner_radius = px;
        self
    }

    /// Replace the children.
    pub fn children(mut self, kids: impl Into<Vec<Element>>) -> Container {
        self.el.children = kids.into();
        self
    }
}

impl_common!(Container);
