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
/// use lumen_widgets::{App, Select};
///
/// let app = App::new(|cx| Select::new(cx, "sel", &["Red", "Green", "Blue"]).into());
/// # lumen_widgets::doc_shot(app, 140.0, 48.0, "select");
/// ```
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
/// use lumen_widgets::{widgets, App, Tooltip};
///
/// let app = App::new(|_| Tooltip::new(widgets::text("hover me"), "A helpful hint").into());
/// # lumen_widgets::doc_shot(app, 160.0, 48.0, "tooltip");
/// ```
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
/// use lumen_widgets::{App, Menu};
///
/// let app = App::new(|_| Menu::new(&["New", "Open", "Save", "Quit"]).into());
/// # lumen_widgets::doc_shot(app, 140.0, 140.0, "menu");
/// ```
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
/// use lumen_widgets::{widgets, App, Wrap};
///
/// let app = App::new(|_| {
///     Wrap::new(vec![widgets::text("alpha"), widgets::text("beta"), widgets::text("gamma")]).into()
/// });
/// # lumen_widgets::doc_shot(app, 180.0, 60.0, "wrap");
/// ```
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
/// use lumen_widgets::{widgets, App, SplitPane};
///
/// let app = App::new(|_| {
///     SplitPane::new(widgets::text("left"), widgets::text("right"), 0.4).into()
/// });
/// # lumen_widgets::doc_shot(app, 220.0, 80.0, "split_pane");
/// ```
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
/// use lumen_widgets::{widgets, App, Modal};
///
/// let app = App::new(|_| {
///     Modal::new(widgets::text("Page behind"), widgets::text("Dialog"), true).into()
/// });
/// # lumen_widgets::doc_shot(app, 220.0, 140.0, "modal");
/// ```
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
/// use lumen_widgets::{widgets, App, PaneGrid};
///
/// let app = App::new(|cx| {
///     PaneGrid::new(cx, "pg", widgets::text("Pane A"), widgets::text("Pane B")).into()
/// });
/// # lumen_widgets::doc_shot(app, 240.0, 100.0, "pane_grid");
/// ```
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
