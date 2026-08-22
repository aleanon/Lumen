//! The frame around an app: top bar, bottom bar, side rail, tabs and
//! pull-to-refresh — plus the platform touch-target minimum they all honour.
//!
//! (SD2: regrouped out of the milestone-named `widgets_m*`/`misc_w2` modules,
//! which recorded WHEN a widget was written rather than what it is.)

use crate::widget::impl_common;
use crate::{BuildCx, Element};
use lumen_core::events::{Key, NamedKey};
use lumen_core::semantics::{Action, Role, ScrollInfo, State as SemState};
use lumen_core::Color;
use lumen_layout::{Dim, Display, Edges, FlexDirection, LayoutStyle};
use lumen_text::TextStyle;
use std::rc::Rc;

/// [`Tabs`] — a tab bar; selected index under `name` (typed form of
/// [`tabs`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{full_width, Tabs, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     full_width(cx, Tabs::new(cx, "tab", &["One", "Two", "Three"]).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 240.0, 60.0, "tabs");
/// ```
///
/// Renders:
///
/// ![Tabs example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/tabs.png)
///
/// The picture above is `src/doc_shots/tabs.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Tabs {
    el: Element,
}

impl Tabs {
    /// A tab bar with its own selected-index state (`name`).
    pub fn new(cx: &BuildCx, name: &str, labels: &[&str]) -> Tabs {
        let el = {
            let selected = cx.signal(name, || 0usize);
            let cur = selected.get(cx.runtime());
            let n = labels.len().max(1);
            let tabs: Vec<Element> = labels
                .iter()
                .enumerate()
                .map(|(i, label)| {
                    let on = i == cur;
                    Element {
                        id: Some(format!("{name}-tab-{i}").into()),
                        role: Role::Tab,
                        label: (*label).to_string(),
                        focusable: true,
                        actions: vec![Action::Click, Action::Focus],
                        states: if on { vec![SemState::Selected] } else { vec![] },
                        background: Some(if on {
                            Color::srgb8(0x1a, 0x73, 0xe8, 0xff)
                        } else {
                            Color::srgb8(0xee, 0xf0, 0xf3, 0xff)
                        }),
                        corner_radius: 4.0,
                        style: LayoutStyle {
                            padding: Edges::all(Dim::px(6.0)),
                            ..LayoutStyle::default()
                        },
                        content: crate::NodeContent::Text(
                            (*label).to_string(),
                            lumen_text::TextStyle {
                                font_size: 14.0,
                                weight: 400.0,
                                color: if on { Color::WHITE } else { Color::BLACK },
                                line_height: None,
                                letter_spacing: 0.0,
                                family: None,
                                features: None,
                                variations: None,
                                italic: false,
                                align: Default::default(),
                            },
                        ),
                        on_click: Some(Rc::new(move |rt| selected.set(rt, i))),
                        // W3: the WAI-ARIA tablist keys — ←/→ move the
                        // selection, Home/End jump to the ends.
                        //
                        // Movement is relative to the *current selection*, not
                        // to this tab's own index: focus does not rove with the
                        // selection (it is keyed by StableId and only Tab moves
                        // it), so keying off `i` would make ← / → depend on
                        // which tab happens to hold focus.
                        on_key: Some(Rc::new(move |rt, ke| {
                            let cur = selected.get(rt);
                            match ke.key {
                                Key::Named(NamedKey::ArrowRight) => selected.set(rt, (cur + 1) % n),
                                Key::Named(NamedKey::ArrowLeft) => {
                                    selected.set(rt, (cur + n - 1) % n)
                                }
                                Key::Named(NamedKey::Home) => selected.set(rt, 0),
                                Key::Named(NamedKey::End) => selected.set(rt, n - 1),
                                _ => {}
                            }
                        })),
                        ..Element::default()
                    }
                })
                .collect();
            Element {
                role: Role::TabList,
                style: LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    column_gap: Dim::px(4.0),
                    ..LayoutStyle::default()
                },
                children: tabs,
                ..Element::default()
            }
        };
        Tabs { el }
    }
}

impl_common!(Tabs);

/// A tab bar with its own selected-index state (`name`).
/// *(Thin shim over [`Tabs`] — the typed form is preferred.)*
pub fn tabs(cx: &BuildCx, name: &str, labels: &[&str]) -> Element {
    Tabs::new(cx, name, labels).into()
}

/// Minimum comfortable touch-target size (logical px).
pub const TOUCH_MIN: f64 = 44.0;

fn touch_style(extra_pad: f32) -> LayoutStyle {
    LayoutStyle {
        min_width: Dim::px(TOUCH_MIN as f32),
        min_height: Dim::px(TOUCH_MIN as f32),
        padding: Edges::all(Dim::px(extra_pad)),
        ..LayoutStyle::default()
    }
}

/// [`BottomNav`] — a full-width bottom navigation row (≥44px targets);
/// selected index under `name` (typed form of [`bottom_nav`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{full_width, BottomNav, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     full_width(cx, BottomNav::new(cx, "nav", &["Home", "Search", "Me"]).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 280.0, 64.0, "bottom_nav");
/// ```
///
/// Renders:
///
/// ![Bottom Nav example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/bottom_nav.png)
///
/// The picture above is `src/doc_shots/bottom_nav.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct BottomNav {
    el: Element,
}

impl BottomNav {
    /// A bottom navigation bar: a full-width row of destination items (≥44px tall).
    /// `name` keys the selected-index signal.
    pub fn new(cx: &BuildCx, name: &str, items: &[&str]) -> BottomNav {
        let el = { nav(cx, name, items, FlexDirection::Row) };
        BottomNav { el }
    }
}

impl_common!(BottomNav);

/// A bottom navigation bar: a full-width row of destination items (≥44px tall).
/// `name` keys the selected-index signal.
/// *(Thin shim over [`BottomNav`] — the typed form is preferred.)*
pub fn bottom_nav(cx: &BuildCx, name: &str, items: &[&str]) -> Element {
    BottomNav::new(cx, name, items).into()
}

/// [`NavigationRail`] — a vertical navigation rail; selected index under
/// `name` (typed form of [`navigation_rail`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, NavigationRail, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, NavigationRail::new(cx, "rail", &["Home", "Files", "Cfg"]).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 110.0, 200.0, "navigation_rail");
/// ```
///
/// Renders:
///
/// ![Navigation Rail example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/navigation_rail.png)
///
/// The picture above is `src/doc_shots/navigation_rail.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct NavigationRail {
    el: Element,
}

impl NavigationRail {
    /// A navigation rail: the vertical equivalent of [`bottom_nav`].
    pub fn new(cx: &BuildCx, name: &str, items: &[&str]) -> NavigationRail {
        let el = { nav(cx, name, items, FlexDirection::Column) };
        NavigationRail { el }
    }
}

impl_common!(NavigationRail);

/// A navigation rail: the vertical equivalent of [`bottom_nav`].
/// *(Thin shim over [`NavigationRail`] — the typed form is preferred.)*
pub fn navigation_rail(cx: &BuildCx, name: &str, items: &[&str]) -> Element {
    NavigationRail::new(cx, name, items).into()
}

fn nav(cx: &BuildCx, name: &str, items: &[&str], dir: FlexDirection) -> Element {
    let selected = cx.signal(name, || 0usize);
    let cur = selected.get(cx.runtime());
    let children: Vec<Element> = items
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let on = i == cur;
            Element {
                role: Role::Tab,
                label: (*label).to_string(),
                focusable: true,
                actions: vec![Action::Click, Action::Focus],
                states: if on { vec![SemState::Selected] } else { vec![] },
                background: Some(if on {
                    Color::srgb8(0x1a, 0x73, 0xe8, 0xff)
                } else {
                    Color::srgb8(0xf5, 0xf6, 0xf8, 0xff)
                }),
                style: LayoutStyle {
                    flex_grow: if dir == FlexDirection::Row { 1.0 } else { 0.0 },
                    ..touch_style(8.0)
                },
                content: crate::NodeContent::Text(
                    (*label).to_string(),
                    TextStyle {
                        font_size: 13.0,
                        weight: 400.0,
                        color: if on { Color::WHITE } else { Color::BLACK },
                        line_height: None,
                        letter_spacing: 0.0,
                        family: None,
                        features: None,
                        variations: None,
                        italic: false,
                        align: Default::default(),
                    },
                ),
                on_click: Some(Rc::new(move |rt| selected.set(rt, i))),
                ..Element::default()
            }
        })
        .collect();
    Element {
        role: Role::TabList,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: dir,
            column_gap: Dim::px(2.0),
            row_gap: Dim::px(2.0),
            width: if dir == FlexDirection::Row {
                Dim::pct(1.0)
            } else {
                Dim::Auto
            },
            ..LayoutStyle::default()
        },
        children,
        ..Element::default()
    }
}

/// [`AppBar`] — a title bar with trailing action items (typed form of
/// [`app_bar`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{widgets, AppBar, Button, BuildCx, Element};
/// use lumen_core::Color;
/// use lumen_layout::Dim;
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let bar = AppBar::new("Inbox", vec![Button::new("Edit").ghost().into()]);
///     // The bar's fill is white; sit it atop a tinted page so it reads as a bar.
///     let mut page = widgets::column(vec![bar.into()]);
///     page.background = Some(Color::srgb8(0xe9, 0xec, 0xf1, 0xff));
///     let win = cx.size();
///     page.style.width = Dim::px(win.width as f32);
///     page.style.height = Dim::px(win.height as f32);
///     page
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 280.0, 96.0, "app_bar");
/// ```
///
/// Renders:
///
/// ![App Bar example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/app_bar.png)
///
/// The picture above is `src/doc_shots/app_bar.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct AppBar {
    el: Element,
}

impl AppBar {
    /// A top app bar: a title with optional trailing action elements (≥44px tall).
    pub fn new(title: impl Into<crate::Text>, actions: Vec<Element>) -> AppBar {
        let el = {
            let title = title.into();
            let (title_s, title_dyn) = title.clone().into_parts();
            let mut children = vec![Element {
                role: Role::Text,
                label: title_s.clone(),
                dyn_text: title_dyn,
                style: LayoutStyle {
                    flex_grow: 1.0,
                    ..LayoutStyle::default()
                },
                content: crate::NodeContent::Text(
                    title_s.clone(),
                    TextStyle {
                        font_size: 20.0,
                        weight: 400.0,
                        color: Color::BLACK,
                        line_height: None,
                        letter_spacing: 0.0,
                        family: None,
                        features: None,
                        variations: None,
                        italic: false,
                        align: Default::default(),
                    },
                ),
                ..Element::default()
            }];
            children.extend(actions);
            Element {
                role: Role::Group,
                label: title_s,
                background: Some(Color::srgb8(0xff, 0xff, 0xff, 0xff)),
                style: LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    column_gap: Dim::px(8.0),
                    min_height: Dim::px(56.0),
                    padding: Edges::all(Dim::px(8.0)),
                    width: Dim::pct(1.0),
                    ..LayoutStyle::default()
                },
                children,
                ..Element::default()
            }
        };
        AppBar { el }
    }
}

impl_common!(AppBar);

/// A top app bar: a title with optional trailing action elements (≥44px tall).
/// *(Thin shim over [`AppBar`] — the typed form is preferred.)*
pub fn app_bar(title: impl Into<crate::Text>, actions: Vec<Element>) -> Element {
    AppBar::new(title, actions).into()
}

/// [`PullToRefresh`] — drag-down-to-refresh wrapper; pull state under
/// `name` (typed form of [`pull_to_refresh`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, widgets, PullToRefresh, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let ptr = PullToRefresh::new(cx, "ptr", 60.0, |_| {}, vec![widgets::text("Pull me down")]);
///     centered(cx, ptr.into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 220.0, 110.0, "pull_to_refresh");
/// ```
///
/// Renders:
///
/// ![Pull To Refresh example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/pull_to_refresh.png)
///
/// The picture above is `src/doc_shots/pull_to_refresh.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct PullToRefresh {
    el: Element,
}

impl PullToRefresh {
    /// A scroll area with pull-to-refresh: dragging down past the top fires
    /// `on_refresh` and surfaces a `busy` state until the `refreshing` signal is
    /// reset. `name` keys both the scroll offset and refresh state.
    pub fn new(
        cx: &BuildCx,
        name: &str,
        threshold: f64,
        on_refresh: impl Fn(&lumen_core::Runtime) + 'static,
        content: Vec<Element>,
    ) -> PullToRefresh {
        let el = {
            let offset = cx.signal(format!("{name}.offset"), || 0.0f64);
            let refreshing = cx.signal(format!("{name}.refreshing"), || false);
            let y = offset.get(cx.runtime());
            let busy = refreshing.get(cx.runtime());
            let on_refresh = Rc::new(on_refresh);

            let indicator = Element {
                role: Role::Progress,
                label: if busy {
                    "Refreshing…"
                } else {
                    "Pull to refresh"
                }
                .to_string(),
                states: if busy { vec![SemState::Busy] } else { vec![] },
                style: LayoutStyle {
                    min_height: Dim::px(24.0),
                    ..LayoutStyle::default()
                },
                content: crate::NodeContent::Text(
                    if busy {
                        "Refreshing…"
                    } else {
                        "Pull to refresh"
                    }
                    .to_string(),
                    TextStyle {
                        font_size: 12.0,
                        weight: 400.0,
                        color: Color::srgb8(0x66, 0x66, 0x66, 0xff),
                        line_height: None,
                        letter_spacing: 0.0,
                        family: None,
                        features: None,
                        variations: None,
                        italic: false,
                        align: Default::default(),
                    },
                ),
                ..Element::default()
            }
            .id(format!("{name}-refresh-indicator"));

            let inner = Element {
                role: Role::ScrollArea,
                scroll: Some(ScrollInfo {
                    x: 0.0,
                    y,
                    max_x: 0.0,
                    max_y: 1e6,
                }),
                style: LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    width: Dim::pct(1.0),
                    ..LayoutStyle::default()
                },
                children: content,
                on_wheel: Some(Rc::new(move |rt, _dx, dy, _mods| {
                    // Wheel delta < 0 is an upward pull; at the top it triggers refresh.
                    let at_top = offset.get(rt) <= 0.0;
                    if at_top && dy <= -threshold && !refreshing.get(rt) {
                        refreshing.set(rt, true);
                        on_refresh(rt);
                    } else {
                        offset.update(rt, |o| *o = (*o + dy).max(0.0));
                    }
                })),
                ..Element::default()
            }
            .id(format!("{name}-scroll"));

            Element {
                role: Role::Group,
                style: LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    width: Dim::pct(1.0),
                    ..LayoutStyle::default()
                },
                children: vec![indicator, inner],
                ..Element::default()
            }
        };
        PullToRefresh { el }
    }
}

impl_common!(PullToRefresh);

/// A scroll area with pull-to-refresh: dragging down past the top fires
/// `on_refresh` and surfaces a `busy` state until the `refreshing` signal is
/// reset. `name` keys both the scroll offset and refresh state.
/// *(Thin shim over [`PullToRefresh`] — the typed form is preferred.)*
pub fn pull_to_refresh(
    cx: &BuildCx,
    name: &str,
    threshold: f64,
    on_refresh: impl Fn(&lumen_core::Runtime) + 'static,
    content: Vec<Element>,
) -> Element {
    PullToRefresh::new(cx, name, threshold, on_refresh, content).into()
}
