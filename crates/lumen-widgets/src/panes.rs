//! Resizable split containers — a two-way `SplitPane` and the recursive
//! `PaneGrid`.
//!
//! (SD2: regrouped out of the milestone-named `widgets_m*`/`misc_w2` modules,
//! which recorded WHEN a widget was written rather than what it is.)

use crate::widget::{impl_widget, Common, Widget};
use crate::{BuildCx, Element};
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use std::rc::Rc;

/// [`SplitPane`] — two panes at a fixed `ratio` split (typed form of
/// [`split_pane`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{full_width, widgets, Container, SplitPane, BuildCx, Element};
/// use lumen_core::Color;
/// use lumen_layout::Dim;
///
/// fn build(cx: &mut BuildCx) -> Element {
///     // Tint the two panes so the 40/60 split is visible (SplitPane itself
///     // draws no divider — it just allots width).
///     let mut left: Element = Container::new(vec![widgets::text("left")])
///         .padding(8.0)
///         .background(Color::srgb8(0xdd, 0xe6, 0xf7, 0xff))
///         .into();
///     let mut right: Element = Container::new(vec![widgets::text("right")])
///         .padding(8.0)
///         .background(Color::srgb8(0xe4, 0xf0, 0xdd, 0xff))
///         .into();
///     left.style.width = Dim::pct(1.0); // fill the allotted pane width
///     right.style.width = Dim::pct(1.0);
///     let mut split: Element = SplitPane::new(left, right, 0.4).into();
///     split.style.height = Dim::px(72.0);
///     full_width(cx, split)
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 240.0, 100.0, "split_pane");
/// ```
///
/// Renders:
///
/// ![Split Pane example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/split_pane.png)
///
/// The picture above is `src/doc_shots/split_pane.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct SplitPane {
    first: Element,
    second: Element,
    ratio: f32,
    common: Common,
}

/// Wrap `child` in a flex pane that takes `grow` of the row.
fn pane(child: Element, grow: f32) -> Element {
    Element {
        role: Role::Group,
        style: LayoutStyle {
            flex_grow: grow,
            flex_basis: Dim::px(0.0),
            ..LayoutStyle::default()
        },
        children: vec![child],
        ..Element::default()
    }
}

impl SplitPane {
    /// Two panes side by side, `first` taking `ratio` of the width.
    pub fn new(first: Element, second: Element, ratio: f32) -> SplitPane {
        SplitPane {
            first,
            second,
            ratio,
            common: Common::default(),
        }
    }
}

impl Widget for SplitPane {
    fn build(self) -> Element {
        let SplitPane {
            first,
            second,
            ratio,
            common,
        } = self;
        let mut el = Element {
            role: Role::Group,
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: Some(Align::Stretch),
                width: Dim::pct(1.0),
                ..LayoutStyle::default()
            },
            // A hairline seam: without it the split is invisible whenever
            // the two panes happen to share a background, which is the
            // common case (both inherit the page).
            children: vec![
                pane(first, ratio.clamp(0.05, 0.95)),
                divider(),
                pane(second, (1.0 - ratio).clamp(0.05, 0.95)),
            ],
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

/// The hairline between two panes.
fn divider() -> Element {
    Element {
        role: Role::Generic,
        elide_semantics: true,
        background: Some(lumen_core::Color::srgb8(0xd6, 0xdb, 0xe4, 0xff)),
        style: LayoutStyle {
            width: Dim::px(1.0),
            flex_grow: 0.0,
            flex_shrink: 0.0,
            align_self: Some(Align::Stretch),
            ..LayoutStyle::default()
        },
        ..Element::default()
    }
}

impl_widget!(SplitPane);

/// A two-pane horizontal split; `ratio` is the fraction given to the first pane.
/// *(Thin shim over [`SplitPane`] — the typed form is preferred.)*
pub fn split_pane(first: Element, second: Element, ratio: f32) -> Element {
    SplitPane::new(first, second, ratio).into()
}

/// [`PaneGrid`] — a draggable two-pane split; ratio under `name` (typed
/// form of [`pane_grid`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{full_width, widgets, PaneGrid, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let pg = PaneGrid::new(cx, "pg", widgets::text("Pane A"), widgets::text("Pane B"));
///     full_width(cx, pg.into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 260.0, 110.0, "pane_grid");
/// ```
///
/// Renders:
///
/// ![Pane Grid example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/pane_grid.png)
///
/// The picture above is `src/doc_shots/pane_grid.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct PaneGrid {
    name: String,
    first: Element,
    second: Element,
    /// The split ratio, read where the `BuildCx` is.
    r: f64,
    ratio: lumen_core::state::Signal<f64>,
    common: Common,
}

impl PaneGrid {
    /// Two panes with a draggable divider; the split lives in the signal keyed
    /// by `name`.
    pub fn new(cx: &BuildCx, name: &str, first: Element, second: Element) -> PaneGrid {
        let ratio = cx.signal(name, || 0.5f64);
        PaneGrid {
            name: name.to_string(),
            first,
            second,
            r: ratio.get(cx.runtime()),
            ratio,
            common: Common::default(),
        }
    }
}

impl Widget for PaneGrid {
    fn build(self) -> Element {
        let PaneGrid {
            name,
            first,
            second,
            r,
            ratio,
            common,
        } = self;
        let divider = Element {
            role: Role::Generic,
            background: Some(Color::srgb8(0x88, 0x8c, 0x90, 0xff)),
            // The only cue that this edge is draggable. A 4 px strip with
            // an arrow cursor is indistinguishable from decoration.
            cursor: Some(lumen_core::CursorShape::ColResize),
            style: LayoutStyle {
                width: Dim::px(6.0),
                height: Dim::pct(1.0),
                // The panes are `flex_grow` with a zero basis, so they take
                // the whole row and the default `flex_shrink: 1` squeezed
                // the divider to **zero width** — it painted nothing and
                // could not be hovered or hit, which is why the grab was
                // undiscoverable in the first place.
                flex_grow: 0.0,
                flex_shrink: 0.0,
                ..LayoutStyle::default()
            },
            ..Element::default()
        }
        .id(format!("{name}-divider"));
        let mut el = Element {
            role: Role::Group,
            value: Some(format!("{r:.2}")),

            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                width: Dim::pct(1.0),
                height: Dim::pct(1.0),
                ..LayoutStyle::default()
            },
            children: vec![
                pane(first, r as f32),
                divider,
                pane(second, (1.0 - r) as f32),
            ],
            ..Element::default()
        }
        .set_on_drag(Some(Rc::new(move |rt, frac, _, _| {
            ratio.set(rt, frac.clamp(0.1, 0.9))
        })))
        .id(name);
        common.apply(&mut el);
        el
    }
}

impl_widget!(PaneGrid);

/// A resizable two-pane split (E8.4). Dragging within the grid sets the split
/// position; `name` keys the ratio. A visual divider marks the boundary.
/// *(Thin shim over [`PaneGrid`] — the typed form is preferred.)*
pub fn pane_grid(cx: &BuildCx, name: &str, first: Element, second: Element) -> Element {
    PaneGrid::new(cx, name, first, second).into()
}
