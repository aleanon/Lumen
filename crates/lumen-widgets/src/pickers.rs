//! Date and time entry — a month calendar and an analogue clock dial, plus the
//! civil-calendar arithmetic they need (leap years, month lengths, and the
//! weekday the month starts on).
//!
//! (SD2: regrouped out of the milestone-named `widgets_m*`/`misc_w2` modules,
//! which recorded WHEN a widget was written rather than what it is.)

use crate::nav_chrome::TOUCH_MIN;
use crate::widget::{impl_widget, Common, Widget};
use crate::{BuildCx, Canvas, Element};
use lumen_core::semantics::{Action, Role, State as SemState};
use lumen_core::{Color, Runtime};
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use std::rc::Rc;

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
/// # lumen_widgets::doc_shot(app, 360.0, 420.0, "date_picker");
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
    name: String,
    /// The selected date, read where the `BuildCx` is. Everything else the
    /// calendar needs is arithmetic over these and the `Copy` handles below.
    yv: i64,
    mv: i64,
    dv: i64,
    year: lumen_core::state::Signal<i64>,
    month: lumen_core::state::Signal<i64>,
    day: lumen_core::state::Signal<i64>,
    common: Common,
}

impl DatePicker {
    /// A date picker rendered as a month calendar (Flutter `showDatePicker`
    /// structure): a month/year header with prev/next arrows, a weekday row, and
    /// a grid of day cells with the selected day highlighted. `name` keys three
    /// signals (`.year`/`.month`/`.day`); value serialises as `YYYY-MM-DD`.
    pub fn new(cx: &BuildCx, name: &str) -> DatePicker {
        let year = cx.signal(format!("{name}.year"), || 2026i64);
        let month = cx.signal(format!("{name}.month"), || 6i64);
        let day = cx.signal(format!("{name}.day"), || 16i64);
        let rt = cx.runtime();
        DatePicker {
            name: name.to_string(),
            yv: year.get(rt),
            mv: month.get(rt),
            dv: day.get(rt),
            year,
            month,
            day,
            common: Common::default(),
        }
    }
}

impl Widget for DatePicker {
    fn build(self) -> Element {
        let DatePicker {
            name,
            yv,
            mv,
            dv,
            year,
            month,
            day,
            common,
        } = self;
        let mut el = calendar(&name, yv, mv, dv, year, month, day);
        common.apply(&mut el);
        el
    }
}

impl_widget!(DatePicker);

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
#[allow(clippy::too_many_arguments)]
fn calendar(
    name: &str,
    yv: i64,
    mv: i64,
    dv: i64,
    year: lumen_core::state::Signal<i64>,
    month: lumen_core::state::Signal<i64>,
    day: lumen_core::state::Signal<i64>,
) -> Element {
    let accent = Color::srgb8(0x1a, 0x73, 0xe8, 0xff);
    let val = format!("{yv:04}-{mv:02}-{dv:02}");

    // Header: ‹  Month YYYY  ›
    let prev = nav_button("‹", &format!("{name}-date-prev"), move |rt| {
        let m = month.get(rt);
        if m <= 1 {
            month.set(rt, 12);
            year.update(rt, |y| *y -= 1);
        } else {
            month.set(rt, m - 1);
        }
    });
    let next = nav_button("›", &format!("{name}-date-next"), move |rt| {
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
/// Clock-dial diameter. Sized so twelve [`TOUCH_MIN`] hour targets fit around
/// the ring without overlapping: the ring circumference `2·π·r` must exceed
/// `12·TOUCH_MIN` (≈528 px), so `r ≥ 84`.
const DIAL: f64 = 240.0;

/// Radius the hour numbers sit at — inset from the rim by half a target.
const DIAL_R_NUMBERS: f64 = DIAL / 2.0 - TOUCH_MIN / 2.0 - 4.0;

/// Wrap `visual` in a transparent [`TOUCH_MIN`]-square hit box.
///
/// Material (and Flutter) draw a calendar day / clock number smaller than the
/// area you can actually hit: the circle is ~36 px but the touch target is
/// ≥44 px. Semantics and `on_click` belong on the **target**, so the audited
/// bounds and the hit box are the target, not the circle.
fn touch_target(visual: Element) -> Element {
    cell_box(visual, TOUCH_MIN as f32, TOUCH_MIN as f32, None, 0.0)
}

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
    let visual = cell_box(t, 36.0, 36.0, selected.then_some(accent), 18.0);
    let mut cell = touch_target(visual);
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
    let visual = cell_box(t, 28.0, 28.0, None, 14.0);
    let mut b = touch_target(visual);
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
/// # lumen_widgets::doc_shot(app, 300.0, 400.0, "time_picker");
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
    /// Kept because the dial's child ids are namespaced under it.
    name: String,
    /// The selected time, read where the `BuildCx` is.
    hv: i64,
    mnv: i64,
    hour: lumen_core::state::Signal<i64>,
    minute: lumen_core::state::Signal<i64>,
    common: Common,
}

impl TimePicker {
    /// A time picker rendered as a clock dial (Flutter `showTimePicker`
    /// structure): a digital `HH:MM` header, a round dial of 1–12 with a hand to
    /// the selected hour, and a compact minute control. Value serialises as
    /// `HH:MM`; `name` keys `.hour`/`.minute`.
    pub fn new(cx: &BuildCx, name: &str) -> TimePicker {
        let hour = cx.signal(format!("{name}.hour"), || 9i64);
        let minute = cx.signal(format!("{name}.minute"), || 30i64);
        let rt = cx.runtime();
        TimePicker {
            name: name.to_string(),
            hv: hour.get(rt),
            mnv: minute.get(rt),
            hour,
            minute,
            common: Common::default(),
        }
    }
}

impl Widget for TimePicker {
    fn build(self) -> Element {
        let TimePicker {
            name,
            hv,
            mnv,
            hour,
            minute,
            common,
        } = self;
        let mut el = clock(&name, hv, mnv, hour, minute);
        common.apply(&mut el);
        el
    }
}

impl_widget!(TimePicker);

/// A clock-dial time picker. Value serialises as `HH:MM`; `name` keys signals.
/// *(Thin shim over [`TimePicker`] — the typed form is preferred.)*
pub fn time_picker(cx: &BuildCx, name: &str) -> Element {
    TimePicker::new(cx, name).into()
}

/// The clock-dial body of [`TimePicker`].
fn clock(
    name: &str,
    hv: i64,
    mnv: i64,
    hour: lumen_core::state::Signal<i64>,
    minute: lumen_core::state::Signal<i64>,
) -> Element {
    let accent = Color::srgb8(0x1a, 0x73, 0xe8, 0xff);
    let dark = Color::srgb8(0x20, 0x24, 0x2a, 0xff);
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
    let mut face: Element = Canvas::new(DIAL, DIAL, move |f, size| {
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
        let x = DIAL / 2.0 + DIAL_R_NUMBERS * a.sin();
        let y = DIAL / 2.0 - DIAL_R_NUMBERS * a.cos();
        let sel = k == hd;
        let mut t = crate::widgets::text(format!("{k}"));
        if let Some(ts) = t.text_style_mut() {
            ts.font_size = 14.0;
            ts.color = if sel { Color::WHITE } else { dark };
        }
        let visual = cell_box(t, 32.0, 32.0, sel.then_some(accent), 16.0);
        let mut b = touch_target(visual);
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
            left: Dim::px((x - TOUCH_MIN / 2.0) as f32),
            top: Dim::px((y - TOUCH_MIN / 2.0) as f32),
            ..Edges::AUTO
        };
        b.on_click = Some(Rc::new(move |rt| hour.set(rt, k)));
        children.push(b.id(format!("{name}-hour-{k}")));
    }
    let dial = Element {
        role: Role::Group,
        style: LayoutStyle {
            position: Position::Relative,
            width: Dim::px(DIAL as f32),
            height: Dim::px(DIAL as f32),
            ..LayoutStyle::default()
        },
        children,
        ..Element::default()
    };

    // Compact minute control (the dial drives the hour).
    let dec = nav_button("−", &format!("{name}-min-dec"), move |rt| {
        minute.update(rt, |x| *x = (*x - 1).rem_euclid(60))
    });
    let inc = nav_button("+", &format!("{name}-min-inc"), move |rt| {
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
