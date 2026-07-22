//! The remaining 02 §10 widgets deferred from M1, completed for the 1.0 freeze
//! (T4.5): Radio, Select, Tooltip, Menu, Grid, Wrap, SplitPane, TextArea. Same
//! `Element`-constructor convention as the other widget sets.

use crate::element::{BuildCx, Element};
use crate::widget::impl_common;
use lumen_core::semantics::{Action, Role, State as SemState};
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, FlexWrap, GridTrack, LayoutStyle};
use lumen_text::TextStyle;
use std::rc::Rc;

/// A radio button in the group keyed by `group`; selecting it sets the group to
/// `value`. Exactly one member of a group is checked.
pub fn radio(cx: &BuildCx, group: &str, value: usize, label: impl Into<String>) -> Element {
    let selected = cx.signal(group, || 0usize);
    let on = selected.get(cx.runtime()) == value;
    let label = label.into();
    Element {
        role: Role::Radio,
        label: label.clone(),
        focusable: true,
        actions: vec![Action::Click, Action::Focus],
        states: if on {
            vec![SemState::Checked]
        } else {
            vec![SemState::Unchecked]
        },
        style: LayoutStyle {
            padding: Edges::all(Dim::px(4.0)),
            ..LayoutStyle::default()
        },
        content: crate::NodeContent::Text(
            format!("{} {label}", if on { "◉" } else { "○" }),
            TextStyle::default(),
        ),
        on_click: Some(Rc::new(move |rt| selected.set(rt, value))),
        ..Element::default()
    }
}

/// [`Select`] — a combo box cycling through `options` on click; selected
/// index under `name` (typed form of [`select`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Select, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Select::new(cx, "sel", &["Red", "Green", "Blue"]).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 160.0, 56.0, "select");
/// ```
///
/// Renders:
///
/// ![Select example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/select.png)
///
/// The picture above is `src/doc_shots/select.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Select {
    el: Element,
}

impl Select {
    /// A select / combo box cycling through `options` on click. `name` keys the
    /// selected index; the semantic value is the current option.
    pub fn new(cx: &BuildCx, name: &str, options: &[&str]) -> Select {
        let el = {
            let idx = cx.signal(name, || 0usize);
            let i = idx.get(cx.runtime()).min(options.len().saturating_sub(1));
            let cur = options.get(i).copied().unwrap_or_default().to_string();
            let n = options.len();
            Element {
                role: Role::ComboBox,
                label: cur.clone(),
                value: Some(cur.clone()),
                focusable: true,
                actions: vec![Action::Click, Action::Focus, Action::SetValue],
                background: Some(Color::srgb8(0xf2, 0xf2, 0xf2, 0xff)),
                corner_radius: 4.0,
                style: LayoutStyle {
                    padding: Edges::all(Dim::px(6.0)),
                    min_width: Dim::px(120.0),
                    ..LayoutStyle::default()
                },
                content: crate::NodeContent::Text(format!("{cur} ▾"), TextStyle::default()),
                on_click: Some(Rc::new(move |rt| {
                    idx.update(rt, |x| *x = (*x + 1) % n.max(1))
                })),
                ..Element::default()
            }
        };
        Select { el }
    }
}

impl_common!(Select);

/// A select / combo box cycling through `options` on click. `name` keys the
/// selected index; the semantic value is the current option.
/// *(Thin shim over [`Select`] — the typed form is preferred.)*
pub fn select(cx: &BuildCx, name: &str, options: &[&str]) -> Element {
    Select::new(cx, name, options).into()
}

/// [`Tooltip`] — wraps `target` with hover-revealed help `text` (typed
/// form of [`tooltip`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, widgets, Tooltip, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Tooltip::new(widgets::text("hover me"), "A helpful hint").into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 180.0, 64.0, "tooltip");
/// ```
///
/// Renders:
///
/// ![Tooltip example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/tooltip.png)
///
/// The picture above is `src/doc_shots/tooltip.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Tooltip {
    el: Element,
}

impl Tooltip {
    /// Wrap `target` with a tooltip whose `text` is exposed to assistive tech.
    pub fn new(target: Element, text: impl Into<String>) -> Tooltip {
        let el = {
            let text = text.into();
            Element {
                role: Role::Group,
                style: LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    ..LayoutStyle::default()
                },
                children: vec![
                    target,
                    Element {
                        role: Role::Tooltip,
                        label: text.clone(),
                        content: crate::NodeContent::Text(
                            text,
                            TextStyle {
                                font_size: 12.0,
                                weight: 400.0,
                                color: Color::srgb8(0x44, 0x44, 0x44, 0xff),
                                line_height: None,
                                letter_spacing: 0.0,
                                family: None,
                            },
                        ),
                        ..Element::default()
                    },
                ],
                ..Element::default()
            }
        };
        Tooltip { el }
    }
}

impl_common!(Tooltip);

/// Wrap `target` with a tooltip whose `text` is exposed to assistive tech.
/// *(Thin shim over [`Tooltip`] — the typed form is preferred.)*
pub fn tooltip(target: Element, text: impl Into<String>) -> Element {
    Tooltip::new(target, text).into()
}

/// [`Menu`] — a vertical list of menu items (typed form of [`menu`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Menu, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Menu::new(&["New", "Open", "Save", "Quit"]).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 160.0, 170.0, "menu");
/// ```
///
/// Renders:
///
/// ![Menu example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/menu.png)
///
/// The picture above is `src/doc_shots/menu.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Menu {
    el: Element,
}

impl Menu {
    /// A vertical menu of selectable items.
    pub fn new(items: &[&str]) -> Menu {
        let el = {
            Element {
                role: Role::Menu,
                style: LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    ..LayoutStyle::default()
                },
                children: items
                    .iter()
                    .map(|t| Element {
                        role: Role::MenuItem,
                        label: (*t).to_string(),
                        focusable: true,
                        actions: vec![Action::Click, Action::Focus],
                        style: LayoutStyle {
                            padding: Edges::all(Dim::px(6.0)),
                            ..LayoutStyle::default()
                        },
                        content: crate::NodeContent::Text((*t).to_string(), TextStyle::default()),
                        ..Element::default()
                    })
                    .collect(),
                ..Element::default()
            }
        };
        Menu { el }
    }
}

impl_common!(Menu);

/// A vertical menu of selectable items.
/// *(Thin shim over [`Menu`] — the typed form is preferred.)*
pub fn menu(items: &[&str]) -> Element {
    Menu::new(items).into()
}

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
    el: Element,
}

impl SplitPane {
    /// A two-pane horizontal split; `ratio` is the fraction given to the first pane.
    pub fn new(first: Element, second: Element, ratio: f32) -> SplitPane {
        let el = {
            let pane = |child: Element, grow: f32| Element {
                role: Role::Group,
                style: LayoutStyle {
                    flex_grow: grow,
                    flex_basis: Dim::px(0.0),
                    ..LayoutStyle::default()
                },
                children: vec![child],
                ..Element::default()
            };
            Element {
                role: Role::Group,
                style: LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: Some(Align::Stretch),
                    width: Dim::pct(1.0),
                    ..LayoutStyle::default()
                },
                children: vec![
                    pane(first, ratio.clamp(0.05, 0.95)),
                    pane(second, (1.0 - ratio).clamp(0.05, 0.95)),
                ],
                ..Element::default()
            }
        };
        SplitPane { el }
    }
}

impl_common!(SplitPane);

/// A two-pane horizontal split; `ratio` is the fraction given to the first pane.
/// *(Thin shim over [`SplitPane`] — the typed form is preferred.)*
pub fn split_pane(first: Element, second: Element, ratio: f32) -> Element {
    SplitPane::new(first, second, ratio).into()
}

/// A multi-line text input. `name` keys the text; typing (including newlines)
/// appends to it.
pub fn text_area(cx: &BuildCx, name: &str, initial: &str) -> Element {
    let value = cx.signal(name, || initial.to_string());
    let v = value.get(cx.runtime());
    let shown = if v.is_empty() {
        " ".to_string()
    } else {
        v.clone()
    };
    Element {
        role: Role::TextInput,
        focusable: true,
        label: v.clone(),
        value: Some(v),
        actions: vec![Action::Focus, Action::SetValue],
        background: Some(Color::srgb8(0xf2, 0xf2, 0xf2, 0xff)),
        corner_radius: 4.0,
        style: LayoutStyle {
            padding: Edges::all(Dim::px(6.0)),
            min_width: Dim::px(160.0),
            min_height: Dim::px(72.0),
            ..LayoutStyle::default()
        },
        content: crate::NodeContent::Text(shown, TextStyle::default()),
        on_text: Some(Rc::new(move |rt, t| {
            let t = t.to_string();
            value.update(rt, |s| s.push_str(&t))
        })),
        ..Element::default()
    }
    .id(name)
}

/// [`Modal`] — `base` content with an optional centered `dialog` overlay
/// when `open` (typed form of [`modal`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{widgets, Container, Modal, BuildCx, Element};
/// use lumen_core::Color;
/// use lumen_layout::Dim;
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let dialog = Container::new(vec![widgets::text("Dialog")])
///         .padding(16.0)
///         .background(Color::WHITE);
///     let mut modal: Element =
///         Modal::new(widgets::text("Page behind"), dialog.into(), true).into();
///     // The modal stacks a full-bleed backdrop over the page, so size it to the
///     // window — then the dialog centers over the whole frame.
///     let win = cx.size();
///     modal.style.width = Dim::px(win.width as f32);
///     modal.style.height = Dim::px(win.height as f32);
///     modal
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 240.0, 160.0, "modal");
/// ```
///
/// Renders:
///
/// ![Modal example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/modal.png)
///
/// The picture above is `src/doc_shots/modal.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Modal {
    el: Element,
}

impl Modal {
    /// A modal overlay (E8.2): when `open`, `dialog` is shown centered over `base`
    /// with a dimmed backdrop; otherwise just `base`.
    pub fn new(base: Element, dialog: Element, open: bool) -> Modal {
        let el = {
            if !open {
                base
            } else {
                let backdrop = Element {
                    role: Role::Group,
                    background: Some(Color::srgb8(0x00, 0x00, 0x00, 0x88)),
                    style: LayoutStyle {
                        position: lumen_layout::Position::Absolute,
                        inset: Edges::all(Dim::px(0.0)),
                        display: Display::Flex,
                        align_items: Some(Align::Center),
                        justify_content: Some(Align::Center),
                        width: Dim::pct(1.0),
                        height: Dim::pct(1.0),
                        ..LayoutStyle::default()
                    },
                    children: vec![dialog],
                    ..Element::default()
                }
                .id("modal-overlay");
                crate::widgets::stack(vec![base, backdrop])
            }
        };
        Modal { el }
    }
}

impl_common!(Modal);

/// A modal overlay (E8.2): when `open`, `dialog` is shown centered over `base`
/// with a dimmed backdrop; otherwise just `base`.
/// *(Thin shim over [`Modal`] — the typed form is preferred.)*
pub fn modal(base: Element, dialog: Element, open: bool) -> Element {
    Modal::new(base, dialog, open).into()
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
    el: Element,
}

impl PaneGrid {
    /// A resizable two-pane split (E8.4). Dragging within the grid sets the split
    /// position; `name` keys the ratio. A visual divider marks the boundary.
    pub fn new(cx: &BuildCx, name: &str, first: Element, second: Element) -> PaneGrid {
        let el = {
            let ratio = cx.signal(name, || 0.5f64);
            let r = ratio.get(cx.runtime());
            let pane = |child: Element, grow: f32| Element {
                role: Role::Group,
                style: LayoutStyle {
                    flex_grow: grow,
                    flex_basis: Dim::px(0.0),
                    ..LayoutStyle::default()
                },
                children: vec![child],
                ..Element::default()
            };
            let divider = Element {
                role: Role::Generic,
                background: Some(Color::srgb8(0x88, 0x8c, 0x90, 0xff)),
                style: LayoutStyle {
                    width: Dim::px(4.0),
                    height: Dim::pct(1.0),
                    ..LayoutStyle::default()
                },
                ..Element::default()
            }
            .id(format!("{name}-divider"));
            Element {
                role: Role::Group,
                value: Some(format!("{:.2}", r)),
                on_drag: Some(Rc::new(move |rt, frac, _, _| {
                    ratio.set(rt, frac.clamp(0.1, 0.9))
                })),
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
            .id(name)
        };
        PaneGrid { el }
    }
}

impl_common!(PaneGrid);

/// A resizable two-pane split (E8.4). Dragging within the grid sets the split
/// position; `name` keys the ratio. A visual divider marks the boundary.
/// *(Thin shim over [`PaneGrid`] — the typed form is preferred.)*
pub fn pane_grid(cx: &BuildCx, name: &str, first: Element, second: Element) -> Element {
    PaneGrid::new(cx, name, first, second).into()
}
