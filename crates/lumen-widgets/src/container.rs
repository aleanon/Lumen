//! [`Container`] — a flex layout box. Its `Element` is built inside
//! [`Container::new`]; modifiers set direction, spacing, padding, alignment, and
//! size.

use crate::widget::{impl_widget, Common, Widget};
use crate::Element;
use lumen_core::semantics::Role;
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle, Position};

/// A flex container (column by default) holding child elements. Use it to group,
/// pad, space, align, and size a subtree.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, widgets, Container, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let row = Container::new(vec![widgets::text("A"), widgets::text("B")])
///         .row()
///         .gap(8.0)
///         .padding(6.0);
///     centered(cx, row.into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 140.0, 60.0, "container");
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
    children: Vec<Element>,
    /// The eight layout knobs the modifiers actually expose.
    ///
    /// Deliberately *not* a whole `LayoutStyle`: that is 256 bytes of which a
    /// container writes eight fields. Holding the eight and assembling the
    /// `LayoutStyle` once, in `build`, is what "only the data the widget needs"
    /// means for a layout box.
    direction: FlexDirection,
    padding: Edges,
    gap: Dim,
    align: Option<Align>,
    justify: Option<Align>,
    width: Dim,
    height: Dim,
    corner_radius: f64,
    /// Overlay layout — children stack absolutely at the top-left.
    stack: bool,
    common: Common,
}

impl Container {
    /// A column container wrapping `children`.
    pub fn new(children: impl Into<Vec<Element>>) -> Container {
        Container {
            children: children.into(),
            direction: FlexDirection::Column,
            padding: Edges::ZERO,
            gap: Dim::px(0.0),
            align: None,
            justify: None,
            width: Dim::Auto,
            height: Dim::Auto,
            corner_radius: 0.0,
            stack: false,
            common: Common::default(),
        }
    }

    /// Lay children out in a row instead of a column.
    pub fn row(mut self) -> Container {
        self.direction = FlexDirection::Row;
        self
    }

    /// Lay children out in a column (the default).
    pub fn column(mut self) -> Container {
        self.direction = FlexDirection::Column;
        self
    }

    /// Overlay layout: children stack at the top-left, last on top (the
    /// typed form of `widgets::stack`).
    ///
    /// The children's own styles are rewritten when the container lowers, not
    /// here — so `.stack()` no longer has to be the last call in the chain to
    /// catch children added by `.children()`.
    pub fn stack(mut self) -> Container {
        self.stack = true;
        self
    }

    /// Uniform padding (px) on all sides.
    pub fn padding(mut self, px: f32) -> Container {
        self.padding = Edges::all(Dim::px(px));
        self
    }

    /// Gap between children on both axes (px).
    pub fn gap(mut self, px: f32) -> Container {
        self.gap = Dim::px(px);
        self
    }

    /// Cross-axis alignment of children (`align-items`).
    pub fn align(mut self, a: Align) -> Container {
        self.align = Some(a);
        self
    }

    /// Main-axis distribution of children (`justify-content`).
    pub fn justify(mut self, a: Align) -> Container {
        self.justify = Some(a);
        self
    }

    /// Fixed width in px.
    pub fn width(mut self, px: f32) -> Container {
        self.width = Dim::px(px);
        self
    }

    /// Fixed height in px.
    pub fn height(mut self, px: f32) -> Container {
        self.height = Dim::px(px);
        self
    }

    /// Fill the parent on both axes.
    pub fn fill(mut self) -> Container {
        self.width = Dim::pct(1.0);
        self.height = Dim::pct(1.0);
        self
    }

    /// Rounded corners (px).
    pub fn corner_radius(mut self, px: f64) -> Container {
        self.corner_radius = px;
        self
    }

    /// Replace the children.
    pub fn children(mut self, kids: impl Into<Vec<Element>>) -> Container {
        self.children = kids.into();
        self
    }
}

impl Widget for Container {
    fn build(self) -> Element {
        let Container {
            children,
            direction,
            padding,
            gap,
            align,
            justify,
            width,
            height,
            corner_radius,
            stack,
            common,
        } = self;
        // A z-stack declares that its children are absolutely positioned; the
        // lowering imposes it as each child is written. It used to walk
        // `children` and write into each one here, which is the pattern that
        // forces a container to receive its children as values.
        let mut el = Element {
            role: Role::Group,
            stacks_children: stack,
            elide_semantics: true,
            corner_radius,
            style: LayoutStyle {
                display: Display::Flex,
                position: if stack {
                    Position::Relative
                } else {
                    LayoutStyle::default().position
                },
                flex_direction: direction,
                padding,
                row_gap: gap,
                column_gap: gap,
                align_items: align,
                justify_content: justify,
                width,
                height,
                ..LayoutStyle::default()
            },
            children,
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Container);
