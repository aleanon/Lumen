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

impl Container {
    /// Split into this node and its children, without ever joining them.
    ///
    /// `Widget::build` puts them back together into a tree; `Direct` never
    /// does — it writes the node, then lowers the children while it is open.
    /// One construction, two consumers, so the two paths cannot drift.
    fn parts(self) -> (Element, Vec<Element>) {
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
        // lowering imposes it as each child is written, rather than this
        // walking `children` and writing into each one — the pattern that
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
            ..Element::default()
        };
        common.apply(&mut el);
        (el, children)
    }
}

impl Widget for Container {
    fn build(self) -> Element {
        let (mut el, children) = self.parts();
        el.children = children;
        el
    }
}

impl crate::Direct for Container {
    /// Native lowering: the node is written, then its children are emitted
    /// while it is open. No `Vec<Element>` is handed to the engine as part of
    /// a tree, and nothing is boxed.
    fn lower_owned(
        self,
        w: &mut dyn crate::NodeWriter,
        parent: Option<crate::NodeIndex>,
        in_overlay: bool,
    ) -> (crate::NodeIndex, crate::LayoutNode) {
        let (el, children) = self.parts();
        w.write_children(el, children, parent, in_overlay)
    }
}

impl_widget!(Container, native);

/// A container whose children are **statements**, not a vector.
///
/// `Container::new(vec![a, b, c])` materializes every child before any of them
/// is written; this writes each one and moves on, so a list of `n` rows costs
/// one node at a time instead of `n` at once. Measured against the `Element`
/// staging tree at **−18.1%** on the lowering path (O0.20).
///
/// ```no_run
/// # use lumen_widgets::{App, Element, widgets, Stack};
/// # fn demo() -> App {
/// App::view(|_cx| {
///     Stack::column(|c| {
///         c.child(widgets::text("first"));
///         for i in 0..3 {
///             c.child(widgets::text(format!("row {i}")));
///         }
///     })
/// })
/// # }
/// ```
pub struct Stack<F> {
    body: F,
    direction: FlexDirection,
    gap: f32,
    padding: f32,
    /// MUT7b: an explicit main/cross size, for statement-form roots that need
    /// a definite containing block (`width: 100%` being every real root).
    width: Option<Dim>,
    height: Option<Dim>,
    /// E2b: cross-axis / main-axis alignment. All four migration tranches
    /// reported the same blocker — every example centres a card, and neither
    /// `Stack` nor `.lss` could express it — so alignment lives here now.
    align: Option<Align>,
    justify: Option<Align>,
    /// E2b: a drop shadow (the card idiom), and this stack's own flex-grow
    /// when it is itself a child (the `fill()` helper idiom).
    shadow: Option<crate::element::Shadow>,
    grow: Option<f32>,
    common: Common,
}

impl<F: FnOnce(&mut crate::Kids)> Stack<F> {
    /// A vertical stack whose children are emitted by `body`.
    pub fn column(body: F) -> Stack<F> {
        Stack {
            body,
            direction: FlexDirection::Column,
            gap: 0.0,
            padding: 0.0,
            width: None,
            height: None,
            align: None,
            justify: None,
            shadow: None,
            grow: None,
            common: Common::default(),
        }
    }

    /// A horizontal stack whose children are emitted by `body`.
    pub fn row(body: F) -> Stack<F> {
        Stack {
            direction: FlexDirection::Row,
            ..Stack::column(body)
        }
    }

    /// Space between children, in logical px.
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    /// Padding inside the stack, in logical px.
    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }

    /// Explicit width (MUT7b) — a statement-form root wants `Dim::pct(1.0)`
    /// for the definite containing block T2's deferred text relies on.
    pub fn width(mut self, w: Dim) -> Self {
        self.width = Some(w);
        self
    }

    /// Explicit height (MUT7b).
    pub fn height(mut self, h: Dim) -> Self {
        self.height = Some(h);
        self
    }

    /// Cross-axis alignment of children (E2b) — `Align::Center` is the
    /// centred-card idiom every example uses.
    pub fn align_items(mut self, a: Align) -> Self {
        self.align = Some(a);
        self
    }

    /// Main-axis distribution of children (E2b).
    pub fn justify_content(mut self, j: Align) -> Self {
        self.justify = Some(j);
        self
    }

    /// Centre children on both axes (E2b) — sugar for the page/card shape.
    pub fn centered(self) -> Self {
        self.align_items(Align::Center)
            .justify_content(Align::Center)
    }

    /// Drop shadow (E2b — the card idiom).
    pub fn shadow(mut self, s: crate::element::Shadow) -> Self {
        self.shadow = Some(s);
        self
    }

    /// This stack's own flex-grow when it is a child (E2b — the `fill()`
    /// helper idiom).
    pub fn grow(mut self, g: f32) -> Self {
        self.grow = Some(g);
        self
    }
}

impl<F: FnOnce(&mut crate::Kids)> crate::Direct for Stack<F> {
    fn lower_owned(
        self,
        w: &mut dyn crate::NodeWriter,
        parent: Option<crate::NodeIndex>,
        in_overlay: bool,
    ) -> (crate::NodeIndex, crate::LayoutNode) {
        let Stack {
            body,
            direction,
            gap,
            padding,
            width,
            height,
            align,
            justify,
            shadow,
            grow,
            common,
        } = self;
        let mut el = Element {
            role: Role::Group,
            elide_semantics: true,
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: direction,
                row_gap: Dim::px(gap),
                column_gap: Dim::px(gap),
                padding: Edges::all(Dim::px(padding)),
                width: width.unwrap_or(Dim::Auto),
                height: height.unwrap_or(Dim::Auto),
                align_items: align,
                justify_content: justify,
                flex_grow: grow.unwrap_or(0.0),
                ..LayoutStyle::default()
            },
            ..Element::default()
        };
        if let Some(sh) = shadow {
            el = el.shadow(sh);
        }
        common.apply(&mut el);
        // E1: the body is `FnOnce` — so eagerly-built children (anything that
        // needed `cx` at build time) can be MOVED into it — adapted through an
        // `Option` because `write_body` needs an object-safe `FnMut`. It runs
        // the body exactly once; the take makes a second call a no-op rather
        // than UB-by-panic in a render path.
        let mut body = Some(body);
        w.write_body(el, parent, in_overlay, &mut move |k: &mut crate::Kids| {
            if let Some(b) = body.take() {
                b(k)
            }
        })
    }
}

// The universal modifiers, by hand: `impl_widget!` takes a concrete type and
// `Stack` is generic over its body closure.
impl<F> Stack<F> {
    /// Set the stable id (tests, the agent, focus, and `.lss` styling).
    pub fn id(mut self, id: impl Into<lumen_core::StableId>) -> Self {
        self.common.set_id(id);
        self
    }
    /// Add a class (for `.lss` selectors).
    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.common.push_class(c);
        self
    }
    /// Background fill.
    pub fn background(mut self, color: lumen_core::Color) -> Self {
        self.common.set_background(color);
        self
    }
    /// Inert and dimmed.
    pub fn disabled(mut self, yes: bool) -> Self {
        self.common.set_disabled(yes);
        self
    }
}
