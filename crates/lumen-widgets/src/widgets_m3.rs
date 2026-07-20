//! M3 mobile widget additions (02 §179): BottomNav, NavigationRail, AppBar,
//! pull-to-refresh, DatePicker, TimePicker. Like the other widgets these are
//! `Element` constructors; stateful ones own a signal keyed by `name`.
//!
//! Interactive controls are sized to at least [`TOUCH_MIN`] logical px so they
//! pass the touch-target audit ([`crate::audit::audit_touch_targets`]).

use crate::element::{BuildCx, Element};
use crate::widget::impl_common;
use lumen_core::semantics::{Action, Role, ScrollInfo, State as SemState};
use lumen_core::Color;
use lumen_layout::{Dim, Display, Edges, FlexDirection, LayoutStyle};
use lumen_text::TextStyle;
use std::rc::Rc;

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
pub struct AppBar {
    el: Element,
}

impl AppBar {
    /// A top app bar: a title with optional trailing action elements (≥44px tall).
    pub fn new(title: impl Into<String>, actions: Vec<Element>) -> AppBar {
        let el = {
            let title = title.into();
            let mut children = vec![Element {
                role: Role::Text,
                label: title.clone(),
                style: LayoutStyle {
                    flex_grow: 1.0,
                    ..LayoutStyle::default()
                },
                content: crate::NodeContent::Text(
                    title.clone(),
                    TextStyle {
                        font_size: 20.0,
                        weight: 400.0,
                        color: Color::BLACK,
                        line_height: None,
                        letter_spacing: 0.0,
                        family: None,
                    },
                ),
                ..Element::default()
            }
            .id("title")];
            children.extend(actions);
            Element {
                role: Role::Group,
                label: title,
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
pub fn app_bar(title: impl Into<String>, actions: Vec<Element>) -> Element {
    AppBar::new(title, actions).into()
}

/// [`PullToRefresh`] — drag-down-to-refresh wrapper; pull state under
/// `name` (typed form of [`pull_to_refresh`]).
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
            let offset = cx.signal(&format!("{name}.offset"), || 0.0f64);
            let refreshing = cx.signal(&format!("{name}.refreshing"), || false);
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
                    },
                ),
                ..Element::default()
            }
            .id("refresh-indicator");

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
            .id("scroll");

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

/// [`DatePicker`] — year/month/day spinners; ISO date under `name`
/// (typed form of [`date_picker`]).
pub struct DatePicker {
    el: Element,
}

impl DatePicker {
    /// A date picker: year / month / day spinners. `name` keys three signals.
    /// Value serialises as `YYYY-MM-DD`.
    pub fn new(cx: &BuildCx, name: &str) -> DatePicker {
        let el = {
            let y = spinner(cx, &format!("{name}.year"), "year", 1970, 2100, 2026);
            let m = spinner(cx, &format!("{name}.month"), "month", 1, 12, 6);
            let d = spinner(cx, &format!("{name}.day"), "day", 1, 31, 16);
            let val = format!("{:04}-{:02}-{:02}", y.0, m.0, d.0);
            picker_group(name, &val, vec![y.1, m.1, d.1])
        };
        DatePicker { el }
    }
}

impl_common!(DatePicker);

/// A date picker: year / month / day spinners. `name` keys three signals.
/// Value serialises as `YYYY-MM-DD`.
/// *(Thin shim over [`DatePicker`] — the typed form is preferred.)*
pub fn date_picker(cx: &BuildCx, name: &str) -> Element {
    DatePicker::new(cx, name).into()
}

/// [`TimePicker`] — hour/minute spinners; `HH:MM` under `name` (typed
/// form of [`time_picker`]).
pub struct TimePicker {
    el: Element,
}

impl TimePicker {
    /// A time picker: hour / minute spinners. Value serialises as `HH:MM`.
    pub fn new(cx: &BuildCx, name: &str) -> TimePicker {
        let el = {
            let h = spinner(cx, &format!("{name}.hour"), "hour", 0, 23, 9);
            let m = spinner(cx, &format!("{name}.minute"), "minute", 0, 59, 30);
            let val = format!("{:02}:{:02}", h.0, m.0);
            picker_group(name, &val, vec![h.1, m.1])
        };
        TimePicker { el }
    }
}

impl_common!(TimePicker);

/// A time picker: hour / minute spinners. Value serialises as `HH:MM`.
/// *(Thin shim over [`TimePicker`] — the typed form is preferred.)*
pub fn time_picker(cx: &BuildCx, name: &str) -> Element {
    TimePicker::new(cx, name).into()
}

fn picker_group(name: &str, value: &str, fields: Vec<Element>) -> Element {
    Element {
        role: Role::Group,
        label: value.to_string(),
        value: Some(value.to_string()),
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            column_gap: Dim::px(12.0),
            ..LayoutStyle::default()
        },
        children: fields,
        ..Element::default()
    }
    .id(name)
}

/// One labelled +/- spinner field. Returns `(current_value, element)`. Buttons
/// carry ids `<key>-dec` / `<key>-inc` so a picker's three fields stay unique.
fn spinner(cx: &BuildCx, sig: &str, key: &str, min: i64, max: i64, init: i64) -> (i64, Element) {
    let value = cx.signal(sig, || init);
    let v = value.get(cx.runtime());
    let dec = tap_button("−", &format!("{key}-dec"), move |rt| {
        value.update(rt, |x| *x = (*x - 1).max(min))
    });
    let inc = tap_button("+", &format!("{key}-inc"), move |rt| {
        value.update(rt, |x| *x = (*x + 1).min(max))
    });
    let el = Element {
        role: Role::Group,
        label: key.to_string(),
        value: Some(format!("{v}")),
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            row_gap: Dim::px(2.0),
            ..LayoutStyle::default()
        },
        children: vec![inc, Element::text(format!("{v}")).id(key), dec],
        ..Element::default()
    };
    (v, el)
}

/// A ≥44px square button (a touch target).
fn tap_button(label: &str, id: &str, on_click: impl Fn(&lumen_core::Runtime) + 'static) -> Element {
    Element {
        role: Role::Button,
        label: label.to_string(),
        focusable: true,
        actions: vec![Action::Click, Action::Focus],
        background: Some(Color::srgb8(0xe8, 0xea, 0xed, 0xff)),
        corner_radius: 6.0,
        style: touch_style(8.0),
        content: crate::NodeContent::Text(
            label.to_string(),
            TextStyle {
                font_size: 18.0,
                weight: 400.0,
                color: Color::BLACK,
                line_height: None,
                letter_spacing: 0.0,
                family: None,
            },
        ),
        on_click: Some(Rc::new(on_click)),
        ..Element::default()
    }
    .id(id)
}
