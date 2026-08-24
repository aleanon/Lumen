//! A dropdown over a long option list stays usable.
//!
//! `PickList` and `Combobox` rendered one row per option, unconditionally. With
//! fifty options that is fifty rows straight off the bottom of the window, and
//! the ones past the edge are unreachable — there is no scroll and nothing to
//! grab. Both now window the panel past `MAX_VISIBLE`, so the panel is a fixed
//! height whatever the option count and the tail is reachable by wheel or bar.

use kurbo::{Rect, Size};
use lumen_core::events::{Event, Modifiers, PointerEvent, WheelEvent};
use lumen_core::geometry::{Point, Vec2};
use lumen_core::semantics::SemanticsNode;
use lumen_core::state::Signal;
use lumen_widgets::{col, App, BuildCx, Headless, PickList};

const MANY: usize = 60;
const ROW_H: f64 = 34.0;
const MAX_VISIBLE: usize = 8;

fn opts() -> Vec<String> {
    (0..MANY).map(|i| format!("option {i:02}")).collect()
}

fn rect_id(n: &SemanticsNode, id: &str) -> Option<Rect> {
    if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
        return Some(n.bounds);
    }
    n.children.iter().find_map(|c| rect_id(c, id))
}

fn open_picker() -> Headless {
    let mut h = App::new(|cx: &mut BuildCx| col![PickList::new(cx, "p", "Pick…", opts()).id("p")])
        .run_headless(Size::new(400.0, 700.0));
    h.pump();
    let b = rect_id(&h.semantics_doc().root, "p-trigger").expect("trigger");
    let p = Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
    h
}

/// The panel is a fixed height whatever the option count.
#[test]
fn a_long_option_list_does_not_grow_the_panel() {
    let h = open_picker();
    let bar = rect_id(&h.semantics_doc().root, "p-scroll-scrollbar")
        .expect("the windowed panel has a scrollbar");
    assert_eq!(
        bar.y1 - bar.y0,
        MAX_VISIBLE as f64 * ROW_H,
        "the viewport shows exactly MAX_VISIBLE rows, not all {MANY}"
    );
}

/// …and only that many rows exist at once, so the cost is independent of the
/// option count.
#[test]
fn only_the_visible_window_is_materialized() {
    let h = open_picker();
    let json = h.semantics_json().to_string();
    let built = (0..MANY)
        .filter(|i| json.contains(&format!("option {i:02}")))
        .count();
    assert!(
        built <= MAX_VISIBLE + 4,
        "expected ~{MAX_VISIBLE} rows plus overscan, got {built}"
    );
}

/// The tail is reachable, which is the whole complaint.
#[test]
fn scrolling_reaches_the_last_option() {
    let mut h = open_picker();
    let bar = rect_id(&h.semantics_doc().root, "p-scroll-scrollbar").expect("bar");
    let inside = Point::new(bar.x0 - 40.0, (bar.y0 + bar.y1) / 2.0);
    for _ in 0..8 {
        h.inject(Event::Wheel(WheelEvent {
            pos: inside,
            delta: Vec2::new(0.0, 400.0),
            modifiers: Modifiers::empty(),
        }));
        h.pump();
    }
    assert!(
        h.semantics_json()
            .to_string()
            .contains(&format!("option {:02}", MANY - 1)),
        "the last option is reachable by scrolling"
    );
}

/// A short list keeps the plain column — no scrollbar, no windowing.
#[test]
fn a_short_list_is_left_alone() {
    let mut h = App::new(|cx: &mut BuildCx| {
        col![PickList::new(cx, "p", "Pick…", ["one", "two", "three"]).id("p")]
    })
    .run_headless(Size::new(400.0, 400.0));
    h.pump();
    let b = rect_id(&h.semantics_doc().root, "p-trigger").unwrap();
    let p = Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
    assert!(rect_id(&h.semantics_doc().root, "p-scroll-scrollbar").is_none());
    assert!(h.semantics_json().to_string().contains("three"));
}

/// Choosing still works through the window.
#[test]
fn a_windowed_row_still_selects() {
    let mut h = open_picker();
    let json = h.semantics_doc();
    fn label_rect(n: &SemanticsNode, label: &str) -> Option<Rect> {
        if n.label == label {
            return Some(n.bounds);
        }
        n.children.iter().find_map(|c| label_rect(c, label))
    }
    let r = label_rect(&json.root, "option 02").expect("a visible row");
    drop(json);
    let p = Point::new((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0);
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
    let sel: Signal<String> = h.runtime().signal("p", String::new);
    assert_eq!(sel.get(h.runtime()), "option 02");
}
