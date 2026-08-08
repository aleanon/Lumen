//! Layout atoms with no state of their own: spacers, dividers, padding, grids,
//! wrapping and alignment boxes.
//!
//! (SD2: regrouped out of the milestone-named `widgets_m*`/`misc_w2` modules,
//! which recorded WHEN a widget was written rather than what it is.)

use crate::widget::impl_common;
use crate::Element;
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_layout::{
    Align as LAlign, Dim, Display, Edges, FlexDirection, FlexWrap, GridTrack, LayoutStyle,
};

/// A standalone alignment container: positions one child inside the
/// available box (the M1 list's `Align`).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, widgets, AlignBox, BuildCx, Element};
/// use lumen_core::Color;
/// use lumen_layout::{Align, Dim};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     // AlignBox fills its parent and positions its child; give it a visible
///     // sized box so you can see "centered" sitting in the middle of it.
///     let mut box_el: Element =
///         AlignBox::new(widgets::text("centered"), Align::Center, Align::Center).into();
///     box_el.background = Some(Color::srgb8(0xec, 0xef, 0xf3, 0xff));
///     box_el.corner_radius = 8.0;
///     box_el.style.width = Dim::px(180.0);
///     box_el.style.height = Dim::px(90.0);
///     centered(cx, box_el)
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 220.0, 130.0, "align_box");
/// ```
///
/// Renders:
///
/// ![Align Box example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/align_box.png)
///
/// The picture above is `src/doc_shots/align_box.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct AlignBox {
    el: Element,
}

impl AlignBox {
    /// Center `child` both ways.
    pub fn center(child: Element) -> AlignBox {
        AlignBox::new(child, LAlign::Center, LAlign::Center)
    }

    /// Explicit cross-axis (`align`) and main-axis (`justify`) placement.
    pub fn new(child: Element, align: LAlign, justify: LAlign) -> AlignBox {
        let el = Element {
            role: Role::Generic,
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                align_items: Some(align),
                justify_content: Some(justify),
                flex_grow: 1.0,
                ..LayoutStyle::default()
            },
            children: vec![child],
            ..Element::default()
        };
        AlignBox { el }
    }
}

impl_common!(AlignBox);

/// A CSS grid with `columns` equal-fraction columns.
pub fn grid(columns: usize, children: Vec<Element>) -> Element {
    Element {
        role: Role::Group,
        style: LayoutStyle {
            display: Display::Grid,
            grid_template_columns: vec![GridTrack::Fr(1.0); columns.max(1)],
            row_gap: Dim::px(4.0),
            column_gap: Dim::px(4.0),
            ..LayoutStyle::default()
        },
        children,
        ..Element::default()
    }
}

/// [`Wrap`] — a flex-wrap row of children (typed form of [`wrap`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, widgets, Wrap, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let items = vec![widgets::text("alpha"), widgets::text("beta"), widgets::text("gamma")];
///     centered(cx, Wrap::new(items).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 200.0, 72.0, "wrap");
/// ```
///
/// Renders:
///
/// ![Wrap example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/wrap.png)
///
/// The picture above is `src/doc_shots/wrap.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Wrap {
    el: Element,
}

impl Wrap {
    /// A flex row that wraps onto new lines.
    pub fn new(children: Vec<Element>) -> Wrap {
        let el = {
            Element {
                role: Role::Group,
                style: LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Dim::px(4.0),
                    row_gap: Dim::px(4.0),
                    ..LayoutStyle::default()
                },
                children,
                ..Element::default()
            }
        };
        Wrap { el }
    }
}

impl_common!(Wrap);

/// A flex row that wraps onto new lines.
/// *(Thin shim over [`Wrap`] — the typed form is preferred.)*
pub fn wrap(children: Vec<Element>) -> Element {
    Wrap::new(children).into()
}

/// Flexible empty space that grows to fill its container.
pub fn spacer() -> Element {
    Element {
        role: Role::Generic,
        elide_semantics: true,
        style: LayoutStyle {
            flex_grow: 1.0,
            ..LayoutStyle::default()
        },
        ..Element::default()
    }
}

/// A horizontal divider line.
pub fn divider() -> Element {
    Element {
        role: Role::Generic,
        background: Some(Color::srgb8(0xd8, 0xdd, 0xe3, 0xff)),
        style: LayoutStyle {
            height: Dim::px(1.0),
            width: Dim::pct(1.0),
            ..LayoutStyle::default()
        },
        ..Element::default()
    }
}

/// Wrap `child` in uniform padding (px).
pub fn padding(px: f32, child: Element) -> Element {
    Element {
        role: Role::Generic,
        elide_semantics: true,
        style: LayoutStyle {
            padding: Edges::all(Dim::px(px)),
            ..LayoutStyle::default()
        },
        children: vec![child],
        ..Element::default()
    }
}
