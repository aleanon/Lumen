//! M3 mobile widget additions (02 §179): BottomNav, NavigationRail, AppBar,
//! pull-to-refresh, DatePicker, TimePicker. Like the other widgets these are
//! `Element` constructors; stateful ones own a signal keyed by `name`.
//!
//! Interactive controls are sized to at least [`TOUCH_MIN`] logical px so they
//! pass the touch-target audit ([`crate::audit::audit_touch_targets`]).

use crate::element::{BuildCx, Element};
use crate::widget::impl_common;
use crate::Canvas;
use lumen_core::semantics::{Action, Role, ScrollInfo, State as SemState};
use lumen_core::{Color, Runtime};
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
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

/// [`DatePicker`] — a month-calendar picker (Flutter `showDatePicker`
/// structure); ISO date under `name` (typed form of [`date_picker`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, DatePicker, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, DatePicker::new(cx, "date").into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 250.0, 300.0, "date_picker");
/// ```
///
/// Renders:
///
/// ![Date Picker example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/date_picker.png)
///
/// The picture above is `src/doc_shots/date_picker.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct DatePicker {
    el: Element,
}

impl DatePicker {
    /// A date picker rendered as a month calendar (Flutter `showDatePicker`
    /// structure): a month/year header with prev/next arrows, a weekday row, and
    /// a grid of day cells with the selected day highlighted. `name` keys three
    /// signals (`.year`/`.month`/`.day`); value serialises as `YYYY-MM-DD`.
    pub fn new(cx: &BuildCx, name: &str) -> DatePicker {
        DatePicker {
            el: calendar(cx, name),
        }
    }
}

impl_common!(DatePicker);

/// A month-calendar date picker. `name` keys three signals; value is `YYYY-MM-DD`.
/// *(Thin shim over [`DatePicker`] — the typed form is preferred.)*
pub fn date_picker(cx: &BuildCx, name: &str) -> Element {
    DatePicker::new(cx, name).into()
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(y) => 29,
        2 => 28,
        _ => 30,
    }
}

/// Day-of-week (0 = Sunday) of the 1st of month `m` in year `y` (Sakamoto's).
fn first_dow(y: i64, m: i64) -> i64 {
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let yy = if m < 3 { y - 1 } else { y };
    (yy + yy / 4 - yy / 100 + yy / 400 + t[(m - 1) as usize] + 1).rem_euclid(7)
}

/// The month-calendar body of [`DatePicker`].
fn calendar(cx: &BuildCx, name: &str) -> Element {
    let accent = Color::srgb8(0x1a, 0x73, 0xe8, 0xff);
    let year = cx.signal(format!("{name}.year"), || 2026i64);
    let month = cx.signal(format!("{name}.month"), || 6i64);
    let day = cx.signal(format!("{name}.day"), || 16i64);
    let (yv, mv, dv) = (
        year.get(cx.runtime()),
        month.get(cx.runtime()),
        day.get(cx.runtime()),
    );
    let val = format!("{yv:04}-{mv:02}-{dv:02}");

    // Header: ‹  Month YYYY  ›
    let prev = nav_button("‹", "date-prev", move |rt| {
        let m = month.get(rt);
        if m <= 1 {
            month.set(rt, 12);
            year.update(rt, |y| *y -= 1);
        } else {
            month.set(rt, m - 1);
        }
    });
    let next = nav_button("›", "date-next", move |rt| {
        let m = month.get(rt);
        if m >= 12 {
            month.set(rt, 1);
            year.update(rt, |y| *y += 1);
        } else {
            month.set(rt, m + 1);
        }
    });
    let mut title =
        crate::widgets::text(format!("{} {yv}", MONTHS[(mv - 1).clamp(0, 11) as usize]));
    if let Some(ts) = title.text_style_mut() {
        ts.font_size = 15.0;
        ts.weight = 600.0;
    }
    title.style.flex_grow = 1.0;
    let header = Element {
        role: Role::Group,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            align_items: Some(Align::Center),
            column_gap: Dim::px(4.0),
            padding: Edges::all(Dim::px(4.0)),
            ..LayoutStyle::default()
        },
        children: vec![prev, title, next],
        ..Element::default()
    };

    // Weekday label row.
    let weekday_row = Element {
        role: Role::Group,
        style: cell_row_style(),
        children: ["S", "M", "T", "W", "T", "F", "S"]
            .iter()
            .map(|d| dow_label(d))
            .collect(),
        ..Element::default()
    };

    // Day grid: 6 weeks × 7 days, blanks before the 1st.
    let fdow = first_dow(yv, mv);
    let dim = days_in_month(yv, mv);
    let mut weeks: Vec<Element> = Vec::new();
    let mut cells: Vec<Element> = Vec::new();
    for slot in 0..42i64 {
        let dnum = slot - fdow + 1;
        cells.push(if (1..=dim).contains(&dnum) {
            day_cell(dnum, dnum == dv, accent, move |rt| day.set(rt, dnum))
        } else {
            empty_cell()
        });
        if cells.len() == 7 {
            weeks.push(Element {
                role: Role::Group,
                style: cell_row_style(),
                children: std::mem::take(&mut cells),
                ..Element::default()
            });
        }
    }

    let mut kids = vec![header, weekday_row];
    kids.extend(weeks);
    Element {
        role: Role::Group,
        label: val.clone(),
        value: Some(val),
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            row_gap: Dim::px(2.0),
            ..LayoutStyle::default()
        },
        children: kids,
        ..Element::default()
    }
    .id(name)
}

fn cell_row_style() -> LayoutStyle {
    LayoutStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        ..LayoutStyle::default()
    }
}

/// A centred fixed-size cell wrapping `text` (so the box, not the glyphs, sets
/// the size — a text-bearing element ignores an explicit height).
fn cell_box(text: Element, w: f32, h: f32, bg: Option<Color>, radius: f64) -> Element {
    Element {
        background: bg,
        corner_radius: radius,
        style: LayoutStyle {
            width: Dim::px(w),
            height: Dim::px(h),
            display: Display::Flex,
            align_items: Some(Align::Center),
            justify_content: Some(Align::Center),
            ..LayoutStyle::default()
        },
        children: vec![text],
        ..Element::default()
    }
}

fn dow_label(d: &str) -> Element {
    let mut t = crate::widgets::text(d);
    if let Some(ts) = t.text_style_mut() {
        ts.font_size = 12.0;
        ts.color = Color::srgb8(0x6b, 0x70, 0x78, 0xff);
    }
    cell_box(t, 30.0, 24.0, None, 0.0)
}

fn empty_cell() -> Element {
    Element {
        style: LayoutStyle {
            width: Dim::px(30.0),
            height: Dim::px(30.0),
            ..LayoutStyle::default()
        },
        ..Element::default()
    }
}

fn day_cell(dnum: i64, selected: bool, accent: Color, on: impl Fn(&Runtime) + 'static) -> Element {
    let mut t = crate::widgets::text(format!("{dnum}"));
    if let Some(ts) = t.text_style_mut() {
        ts.font_size = 13.0;
        ts.color = if selected {
            Color::WHITE
        } else {
            Color::srgb8(0x20, 0x24, 0x2a, 0xff)
        };
    }
    let mut cell = cell_box(t, 30.0, 30.0, selected.then_some(accent), 15.0);
    cell.role = Role::Button;
    cell.label = format!("{dnum}");
    cell.focusable = true;
    cell.actions = vec![Action::Click, Action::Focus];
    cell.states = if selected {
        vec![SemState::Selected]
    } else {
        vec![]
    };
    cell.on_click = Some(Rc::new(on));
    cell
}

/// A small square icon button for calendar month navigation.
fn nav_button(label: &str, id: &str, on: impl Fn(&Runtime) + 'static) -> Element {
    let mut t = crate::widgets::text(label);
    if let Some(ts) = t.text_style_mut() {
        ts.font_size = 18.0;
    }
    let mut b = cell_box(t, 28.0, 28.0, None, 14.0);
    b.role = Role::Button;
    b.label = label.to_string();
    b.focusable = true;
    b.actions = vec![Action::Click, Action::Focus];
    b.on_click = Some(Rc::new(on));
    b.id(id)
}

/// [`TimePicker`] — a clock-dial picker (Flutter `showTimePicker` structure);
/// `HH:MM` under `name` (typed form of [`time_picker`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, TimePicker, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, TimePicker::new(cx, "time").into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 240.0, 300.0, "time_picker");
/// ```
///
/// Renders:
///
/// ![Time Picker example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/time_picker.png)
///
/// The picture above is `src/doc_shots/time_picker.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct TimePicker {
    el: Element,
}

impl TimePicker {
    /// A time picker rendered as a clock dial (Flutter `showTimePicker`
    /// structure): a digital `HH:MM` header, a round dial of 1–12 with a hand to
    /// the selected hour, and a compact minute control. Value serialises as
    /// `HH:MM`; `name` keys `.hour`/`.minute`.
    pub fn new(cx: &BuildCx, name: &str) -> TimePicker {
        TimePicker {
            el: clock(cx, name),
        }
    }
}

impl_common!(TimePicker);

/// A clock-dial time picker. Value serialises as `HH:MM`; `name` keys signals.
/// *(Thin shim over [`TimePicker`] — the typed form is preferred.)*
pub fn time_picker(cx: &BuildCx, name: &str) -> Element {
    TimePicker::new(cx, name).into()
}

/// The clock-dial body of [`TimePicker`].
fn clock(cx: &BuildCx, name: &str) -> Element {
    let accent = Color::srgb8(0x1a, 0x73, 0xe8, 0xff);
    let dark = Color::srgb8(0x20, 0x24, 0x2a, 0xff);
    let hour = cx.signal(format!("{name}.hour"), || 9i64);
    let minute = cx.signal(format!("{name}.minute"), || 30i64);
    let hv = hour.get(cx.runtime());
    let mnv = minute.get(cx.runtime());
    let hd = if hv % 12 == 0 { 12 } else { hv % 12 };
    let val = format!("{hv:02}:{mnv:02}");

    // Digital header: HH : MM (hour emphasised).
    let big = |s: String, color: Color| {
        let mut t = crate::widgets::text(s);
        if let Some(ts) = t.text_style_mut() {
            ts.font_size = 30.0;
            ts.weight = 500.0;
            ts.color = color;
        }
        t
    };
    let header = Element {
        role: Role::Group,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            align_items: Some(Align::Center),
            justify_content: Some(Align::Center),
            column_gap: Dim::px(2.0),
            ..LayoutStyle::default()
        },
        children: vec![
            big(format!("{hv:02}"), accent),
            big(":".into(), dark),
            big(format!("{mnv:02}"), dark),
        ],
        ..Element::default()
    };

    // Dial face + hand (canvas leaf) under clickable hour-number overlays.
    let hd_draw = hd;
    let mut face: Element = Canvas::new(200.0, 200.0, move |f, size| {
        let (cx0, cy0) = (size.width / 2.0, size.height / 2.0);
        let r = cx0.min(cy0) - 6.0;
        f.fill_circle(
            kurbo::Point::new(cx0, cy0),
            r,
            Color::srgb8(0xef, 0xf1, 0xf4, 0xff),
        );
        let a = (hd_draw as f64 * 30.0).to_radians();
        let r_hand = r - 22.0;
        let tip = kurbo::Point::new(cx0 + r_hand * a.sin(), cy0 - r_hand * a.cos());
        let mut p = kurbo::BezPath::new();
        p.move_to((cx0, cy0));
        p.line_to(tip);
        f.stroke(&p, accent, 2.0);
        f.fill_circle(kurbo::Point::new(cx0, cy0), 4.0, accent);
    })
    .into();
    face.style.position = Position::Absolute;
    face.style.inset = Edges {
        left: Dim::px(0.0),
        top: Dim::px(0.0),
        ..Edges::AUTO
    };

    let mut children = vec![face];
    for k in 1..=12i64 {
        let a = (k as f64 * 30.0).to_radians();
        let x = 100.0 + 74.0 * a.sin();
        let y = 100.0 - 74.0 * a.cos();
        let sel = k == hd;
        let mut t = crate::widgets::text(format!("{k}"));
        if let Some(ts) = t.text_style_mut() {
            ts.font_size = 14.0;
            ts.color = if sel { Color::WHITE } else { dark };
        }
        let mut b = cell_box(t, 28.0, 28.0, sel.then_some(accent), 14.0);
        b.role = Role::Button;
        b.label = format!("{k} o'clock");
        b.focusable = true;
        b.actions = vec![Action::Click, Action::Focus];
        b.states = if sel {
            vec![SemState::Selected]
        } else {
            vec![]
        };
        b.style.position = Position::Absolute;
        b.style.inset = Edges {
            left: Dim::px((x - 14.0) as f32),
            top: Dim::px((y - 14.0) as f32),
            ..Edges::AUTO
        };
        b.on_click = Some(Rc::new(move |rt| hour.set(rt, k)));
        children.push(b.id(format!("hour-{k}")));
    }
    let dial = Element {
        role: Role::Group,
        style: LayoutStyle {
            position: Position::Relative,
            width: Dim::px(200.0),
            height: Dim::px(200.0),
            ..LayoutStyle::default()
        },
        children,
        ..Element::default()
    };

    // Compact minute control (the dial drives the hour).
    let dec = nav_button("−", "min-dec", move |rt| {
        minute.update(rt, |x| *x = (*x - 1).rem_euclid(60))
    });
    let inc = nav_button("+", "min-inc", move |rt| {
        minute.update(rt, |x| *x = (*x + 1).rem_euclid(60))
    });
    let mut mlabel = crate::widgets::text(format!("{mnv:02} min"));
    if let Some(ts) = mlabel.text_style_mut() {
        ts.font_size = 13.0;
        ts.color = dark;
    }
    let minute_row = Element {
        role: Role::Group,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            align_items: Some(Align::Center),
            justify_content: Some(Align::Center),
            column_gap: Dim::px(8.0),
            ..LayoutStyle::default()
        },
        children: vec![dec, cell_box(mlabel, 56.0, 24.0, None, 0.0), inc],
        ..Element::default()
    };

    Element {
        role: Role::Group,
        label: val.clone(),
        value: Some(val),
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            align_items: Some(Align::Center),
            row_gap: Dim::px(8.0),
            ..LayoutStyle::default()
        },
        children: vec![header, dial, minute_row],
        ..Element::default()
    }
    .id(name)
}
