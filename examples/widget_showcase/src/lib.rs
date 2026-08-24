//! widget_showcase — every built-in Lumen widget, one at a time.
//!
//! A dropdown pinned to the top centre lists the whole widget catalog, grouped
//! by kind; picking one drops it, seeded with real data, into the middle of the
//! rest of the screen. Widgets that carry state also print a live readout under
//! the picker, so you can watch a slider drag or a checkbox toggle land in the
//! store.
//!
//! Two layout facts drive the structure of `build`:
//!
//! * `Element::overlay` moves a subtree into the final *paint* pass, but
//!   hit-testing still follows document order — later siblings win. So the
//!   header is the **last** child (positioned back to the top with
//!   `Position::Absolute`), or the stage would swallow clicks on the open
//!   dropdown wherever the two overlap.
//! * A root element is content-sized. Both layers are therefore sized from
//!   `cx.size()`, and a window resize rebuilds them.
use lumen_core::semantics::Role;
use lumen_core::state::Signal;
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use lumen_widgets::element::Shadow;
use lumen_widgets::{bind, widgets, App, BuildCx, Element, Label, Scrollable};

pub mod catalog;

pub use catalog::{Entry, Place};

/// Build the showcase app.
pub fn main_app() -> App {
    App::new(build)
}

/// Height of the pinned header band.
const HEADER_H: f32 = 116.0;
/// Width of the picker trigger and its panel.
const PICKER_W: f32 = 300.0;
/// Visible height of the (scrolling) dropdown panel.
const PANEL_H: f64 = 420.0;
/// Height of one selectable row in the panel.
const ROW_H: f64 = 30.0;
/// Height of a group heading row in the panel.
const HEAD_H: f64 = 28.0;
/// Stage padding on the left, right and bottom edges.
const PAD: f32 = 28.0;

fn page() -> Color {
    Color::srgb8(0xf4, 0xf6, 0xfa, 0xff)
}
fn band() -> Color {
    Color::WHITE
}
fn ink() -> Color {
    Color::srgb8(0x1b, 0x22, 0x30, 0xff)
}
fn muted() -> Color {
    Color::srgb8(0x6b, 0x74, 0x88, 0xff)
}
fn accent() -> Color {
    Color::srgb8(0x2f, 0x6b, 0xff, 0xff)
}
fn hilite() -> Color {
    Color::srgb8(0xed, 0xf2, 0xff, 0xff)
}
fn hairline() -> Color {
    Color::srgb8(0xdf, 0xe4, 0xed, 0xff)
}

/// The stage's content box in window coordinates: `(x, y, w, h)`.
///
/// Most demos never need this — they are laid out by flexbox like anything
/// else. `Grid` does: it virtualizes during *build*, before layout has run, so
/// it has to be told the window-space rect it will occupy (it otherwise assumes
/// the whole surface and lays out cells for a box far larger than it gets).
pub fn stage_content_rect(size: lumen_core::geometry::Size) -> (f64, f64, f64, f64) {
    (
        PAD as f64,
        HEADER_H as f64,
        (size.width - 2.0 * PAD as f64).max(0.0),
        (size.height - HEADER_H as f64 - PAD as f64).max(0.0),
    )
}

/// The signal holding the selected widget's [`Entry::name`].
fn selection(cx: &BuildCx) -> Signal<String> {
    cx.signal("widget", || "Button".to_string())
}

fn build(cx: &mut BuildCx) -> Element {
    let win = cx.size();
    let name = selection(cx).get(cx.runtime());
    let entry = catalog::find(&name);

    // Document order matters: the stage first, the header last, so the header
    // (and the dropdown panel inside it) wins both paint and hit-testing.
    let stage = stage(cx, entry, win.width as f32, win.height as f32);
    let header = header(cx, entry, win.width as f32);

    let mut root = Element {
        role: Role::Generic,
        elide_semantics: true,
        background: Some(page()),
        style: LayoutStyle {
            position: Position::Relative,
            width: Dim::px(win.width as f32),
            height: Dim::px(win.height as f32),
            ..LayoutStyle::default()
        },
        children: vec![stage, header],
        ..Element::default()
    };
    root.id = Some("showcase".into());
    root
}

// ------------------------------------------------------------------ header ---

/// The pinned band: the picker, the entry's blurb, and its live state readout.
fn header(cx: &mut BuildCx, entry: &'static Entry, w: f32) -> Element {
    let mut blurb = widgets::text(entry.blurb);
    if let Some(ts) = blurb.text_style_mut() {
        ts.font_size = 12.5;
        ts.color = muted();
    }

    let mut caption: Vec<Element> = vec![blurb];
    if let Some(f) = entry.status {
        // A binding, not a value: the readout patches on every store change
        // without re-running this closure or relaying out the stage.
        let mut live = widgets::text(bind!(rt => f(rt)));
        if let Some(ts) = live.text_style_mut() {
            ts.font_size = 12.5;
            ts.weight = 600.0;
            ts.color = accent();
        }
        caption.push(live.id("status"));
    }

    let mut caption_row = widgets::row(caption);
    caption_row.style.column_gap = Dim::px(14.0);
    caption_row.style.align_items = Some(Align::Center);

    let mut band = Element {
        role: Role::Generic,
        background: Some(band()),
        shadow: Some(Shadow::soft()),
        style: LayoutStyle {
            position: Position::Absolute,
            inset: Edges {
                left: Dim::px(0.0),
                top: Dim::px(0.0),
                ..Edges::AUTO
            },
            width: Dim::px(w),
            height: Dim::px(HEADER_H),
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            align_items: Some(Align::Center),
            justify_content: Some(Align::Center),
            row_gap: Dim::px(10.0),
            padding: Edges::all(Dim::px(14.0)),
            ..LayoutStyle::default()
        },
        children: vec![picker(cx, entry), caption_row],
        ..Element::default()
    };
    band.id = Some("header".into());
    band
}

// ------------------------------------------------------------------ picker ---

/// The dropdown: a trigger plus, while open, an overlaid scrolling panel of
/// every widget in the catalog.
///
/// `PickList` would be the obvious choice, but its panel is an uncapped column
/// — sixty options would render ~1 800 px of rows straight off the bottom of
/// the window. This one puts the rows in a [`Scrollable`] instead.
fn picker(cx: &mut BuildCx, entry: &'static Entry) -> Element {
    let selected = selection(cx);
    let open = cx.signal("picker-open", || false);
    let is_open = open.get(cx.runtime());

    let mut label = widgets::text(entry.name);
    if let Some(ts) = label.text_style_mut() {
        ts.font_size = 15.0;
        ts.weight = 600.0;
        ts.color = ink();
    }
    label.style.flex_grow = 1.0;

    let mut chevron = widgets::text(if is_open { "▴" } else { "▾" });
    if let Some(ts) = chevron.text_style_mut() {
        ts.font_size = 13.0;
        ts.color = muted();
    }

    let mut trigger = widgets::row(vec![label, chevron]);
    trigger.role = Role::Button;
    trigger.label = format!("Widget: {}", entry.name);
    trigger.focusable = true;
    // `widgets::row` elides itself from semantics; this row *is* the control,
    // so it has to stay visible to selectors, focus and assistive tech.
    trigger.elide_semantics = false;
    trigger.id = Some("widget-picker".into());
    trigger.background = Some(band());
    trigger.corner_radius = 9.0;
    trigger.style.width = Dim::px(PICKER_W);
    trigger.style.height = Dim::px(38.0);
    trigger.style.align_items = Some(Align::Center);
    trigger.style.column_gap = Dim::px(8.0);
    trigger.style.padding = Edges {
        left: Dim::px(14.0),
        right: Dim::px(12.0),
        top: Dim::px(0.0),
        bottom: Dim::px(0.0),
    };
    trigger.border = Some(lumen_render::Border {
        width: 1.0,
        color: hairline(),
    });
    trigger.on_click = Some(std::rc::Rc::new(move |rt| open.update(rt, |o| *o = !*o)));

    let mut children = vec![trigger];
    if is_open {
        let mut rows: Vec<Element> = Vec::new();
        let mut content_h = 0.0f64;
        for group in catalog::groups() {
            rows.push(group_heading(group.title));
            content_h += HEAD_H;
            for e in group.entries {
                rows.push(option_row(e, e.name == entry.name, selected, open));
                content_h += ROW_H;
            }
        }

        let mut list: Element =
            Scrollable::new(cx, "picker-scroll", PANEL_H, content_h, rows).into();
        list.style.width = Dim::px(PICKER_W);

        let mut panel = Element {
            role: Role::Menu,
            background: Some(band()),
            corner_radius: 10.0,
            shadow: Some(Shadow::soft()),
            // Paint above everything and escape ancestor clips.
            overlay: true,
            style: LayoutStyle {
                position: Position::Absolute,
                inset: Edges {
                    left: Dim::px(0.0),
                    top: Dim::pct(1.0),
                    ..Edges::AUTO
                },
                margin: Edges {
                    top: Dim::px(6.0),
                    ..Edges::ZERO
                },
                width: Dim::px(PICKER_W),
                padding: Edges::all(Dim::px(6.0)),
                ..LayoutStyle::default()
            },
            children: vec![list],
            ..Element::default()
        };
        panel.id = Some("picker-panel".into());
        // Light dismiss: a click outside, or Escape, closes the panel.
        panel.on_dismiss = Some(std::rc::Rc::new(move |rt| open.set(rt, false)));
        children.push(panel);
    }

    Element {
        role: Role::Group,
        elide_semantics: true,
        style: LayoutStyle {
            position: Position::Relative,
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Dim::px(PICKER_W),
            ..LayoutStyle::default()
        },
        children,
        ..Element::default()
    }
}

fn group_heading(title: &'static str) -> Element {
    let mut t = widgets::text(title);
    if let Some(ts) = t.text_style_mut() {
        ts.font_size = 10.5;
        ts.weight = 700.0;
        ts.letter_spacing = 1.0;
        ts.color = muted();
    }
    let mut row = widgets::row(vec![t]);
    row.style.height = Dim::px(HEAD_H as f32);
    row.style.width = Dim::pct(1.0);
    row.style.align_items = Some(Align::Center);
    row.style.padding = Edges {
        left: Dim::px(10.0),
        right: Dim::px(10.0),
        top: Dim::px(0.0),
        bottom: Dim::px(0.0),
    };
    row
}

fn option_row(
    entry: &'static Entry,
    current: bool,
    selected: Signal<String>,
    open: Signal<bool>,
) -> Element {
    let mut t = widgets::text(entry.name);
    if let Some(ts) = t.text_style_mut() {
        ts.font_size = 13.5;
        ts.weight = if current { 700.0 } else { 400.0 };
        ts.color = if current { accent() } else { ink() };
    }

    let mut row = widgets::row(vec![t]);
    row.role = Role::MenuItem;
    row.label = entry.name.to_string();
    row.elide_semantics = false;
    row.id = Some(format!("opt-{}", entry.slug).into());
    row.background = Some(if current { hilite() } else { band() });
    row.corner_radius = 6.0;
    row.style.height = Dim::px(ROW_H as f32);
    row.style.width = Dim::pct(1.0);
    row.style.align_items = Some(Align::Center);
    row.style.padding = Edges {
        left: Dim::px(10.0),
        right: Dim::px(10.0),
        top: Dim::px(0.0),
        bottom: Dim::px(0.0),
    };
    let name = entry.name;
    row.on_click = Some(std::rc::Rc::new(move |rt| {
        // Only `Copy` state is captured: two signal handles and a `&'static str`.
        selected.set(rt, name.to_string());
        open.set(rt, false);
    }));
    row
}

// ------------------------------------------------------------------- stage ---

/// The area under the header, holding exactly one seeded widget.
///
/// It is sized to the *whole* window with `HEADER_H` of top padding rather than
/// offset below the band, so that absolutely-positioned descendants (a `Sheet`
/// scrim, a `Modal` backdrop) which resolve against this positioned ancestor
/// still cover the full window the way they would in a real app.
fn stage(cx: &mut BuildCx, entry: &'static Entry, w: f32, h: f32) -> Element {
    let demo = (entry.build)(cx);

    let (align, justify, top_pad) = match entry.place {
        Place::Center => (Align::Center, Align::Center, 0.0),
        Place::Wide => (Align::Stretch, Align::Center, 0.0),
        Place::Top => (Align::Center, Align::Start, 24.0),
        // The overlay demos size themselves to the window; stage padding would
        // shift the containing block their scrim resolves against.
        Place::Overlay => (Align::Stretch, Align::Start, 0.0),
    };
    let pad = if entry.place == Place::Overlay {
        Edges::ZERO
    } else {
        Edges {
            left: Dim::px(PAD),
            right: Dim::px(PAD),
            top: Dim::px(HEADER_H + top_pad),
            bottom: Dim::px(PAD),
        }
    };

    let mut stage = Element {
        role: Role::Generic,
        style: LayoutStyle {
            position: Position::Absolute,
            inset: Edges {
                left: Dim::px(0.0),
                top: Dim::px(0.0),
                ..Edges::AUTO
            },
            width: Dim::px(w),
            height: Dim::px(h),
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            align_items: Some(align),
            justify_content: Some(justify),
            padding: pad,
            ..LayoutStyle::default()
        },
        children: vec![demo],
        ..Element::default()
    };
    stage.id = Some("stage".into());
    stage
}

/// A heading used by the tests to prove the stage swapped.
#[doc(hidden)]
pub fn heading(name: &str) -> Element {
    Label::new(name.to_string()).into()
}
