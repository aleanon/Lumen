//! `on_click` fires on the *release*, not the press.
//!
//! The press-fires-click rule was invisible with a mouse and wrong with a
//! finger: touch panning (added just before this) means a finger that presses a
//! row and then drags to scroll had *already activated that row* before it
//! moved a pixel. Mercurium hit the same class of bug from the other side — a
//! tap that landed on a tile also hit the Send pill behind it.
//!
//! Two rules, and both are needed:
//!
//! * the release must land back on the node the press picked, and
//! * a touch that travels past the slop stops being a candidate at all.
//!
//! The second is not redundant. When a list scrolls under the finger the row
//! travels *with* it, so at release the finger is still over the very same row —
//! position alone cannot tell a tap from a drag.

use kurbo::Size;
use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
use lumen_core::geometry::Point;
use lumen_core::state::Signal;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, BuildCx, Element, VirtualList};
use std::rc::Rc;

const W: f64 = 300.0;
const VIEWPORT: f64 = 200.0;

fn ev(pos: Point, kind: PointerKind) -> PointerEvent {
    PointerEvent {
        pos,
        button: PointerButton::Left,
        pointer: kind,
        modifiers: Default::default(),
        click_count: 1,
    }
}

/// A scrollable list whose rows are individually clickable — the shape the
/// press-vs-drag ambiguity actually shows up in.
fn clickable_rows() -> impl Fn(&mut BuildCx) -> Element {
    |cx: &mut BuildCx| {
        let hits: Signal<u32> = cx.signal("hits", || 0);
        let vl = VirtualList::new(cx, "vl", 1000, 20.0, VIEWPORT, move |i| {
            // A container, not a bare `text`: a text-bearing element sizes to
            // its glyphs, so it would shrink-wrap to ~42 px and the press
            // coordinates below would sail past it.
            let mut row = widgets::column(vec![widgets::text(format!("row {i}"))]);
            row.on_click = Some(Rc::new(move |rt| hits.update(rt, |v| *v += 1)));
            row.id(format!("row-{i}"))
        });
        let mut root = widgets::column(vec![vl.into()]);
        root.style.width = Dim::px(W as f32);
        root.id("root")
    }
}

fn hits(h: &lumen_widgets::Headless) -> u32 {
    let s: Signal<u32> = h.runtime().signal("hits", || 0);
    s.get(h.runtime())
}

/// Two buttons side by side, so a press on one can be released on the other.
fn two_buttons() -> impl Fn(&mut BuildCx) -> Element {
    |cx: &mut BuildCx| {
        let a: Signal<u32> = cx.signal("a", || 0);
        let b: Signal<u32> = cx.signal("b", || 0);
        let mk = |label: &str, sig: Signal<u32>| {
            // Wrapped for the same reason as the rows above: `height` on a text
            // element is ignored, so a bare label would be ~21 px tall.
            let mut e = widgets::column(vec![widgets::text(label.to_string())]);
            e.style.width = Dim::px(140.0);
            e.style.height = Dim::px(100.0);
            e.on_click = Some(Rc::new(move |rt| sig.update(rt, |v| *v += 1)));
            e
        };
        let mut root = widgets::row(vec![mk("A", a), mk("B", b)]);
        root.style.width = Dim::px(W as f32);
        root.id("root")
    }
}

/// The press arms nothing observable; the release is what activates.
#[test]
fn a_press_alone_activates_nothing() {
    let mut h = App::new(clickable_rows()).run_headless(Size::new(W, VIEWPORT));
    h.pump();
    let p = Point::new(100.0, 30.0);

    h.inject(Event::PointerDown(ev(p, PointerKind::Mouse)));
    h.pump();
    assert_eq!(hits(&h), 0, "the press must not activate the row");

    h.inject(Event::PointerUp(ev(p, PointerKind::Mouse)));
    h.pump();
    assert_eq!(hits(&h), 1, "the release activates it");
}

/// The bug this exists for: dragging the list must not activate the row the
/// finger started on — even though that row is still under the finger at
/// release, because it scrolled along with it.
#[test]
fn a_finger_that_scrolls_does_not_click_the_row_it_started_on() {
    let mut h = App::new(clickable_rows()).run_headless(Size::new(W, VIEWPORT));
    h.pump();
    let x = 100.0;

    h.inject(Event::PointerDown(ev(
        Point::new(x, 150.0),
        PointerKind::Touch,
    )));
    // Pumped between moves, which is the shape a device produces: the shell
    // injects as events arrive and pumps per frame. It matters here — the list
    // re-lays-out after each move, so the row that was under the finger at the
    // press is still under it at the release, 80 px later. Batch all four moves
    // into one pump and the release hit-tests against the *pre-drag* layout,
    // lands on a different row, and the test passes without the slop rule
    // doing any work at all.
    for y in [130.0, 110.0, 90.0, 70.0] {
        h.inject(Event::PointerMove(ev(Point::new(x, y), PointerKind::Touch)));
        h.pump();
    }
    h.inject(Event::PointerUp(ev(
        Point::new(x, 70.0),
        PointerKind::Touch,
    )));
    h.pump();

    let y: Signal<f64> = h.runtime().signal("vl", || 0.0);
    assert_eq!(y.get(h.runtime()), 80.0, "the drag still scrolls");
    let row7 = h
        .node_bounds_by_id("row-7")
        .expect("row 7 is still mounted");
    assert!(
        row7.y0 <= 70.0 && 70.0 <= row7.y1,
        "precondition: the row pressed at y=150 rode the scroll and is under the \
         finger again at y=70 ({row7:?}) — so position alone cannot rule this \
         click out, and only the slop can"
    );
    assert_eq!(hits(&h), 0, "and must not activate the row it began on");
}

/// A tap is never perfectly still. Movement under the slop still activates.
#[test]
fn a_jittery_tap_still_clicks() {
    let mut h = App::new(clickable_rows()).run_headless(Size::new(W, VIEWPORT));
    h.pump();
    let x = 100.0;

    h.inject(Event::PointerDown(ev(
        Point::new(x, 150.0),
        PointerKind::Touch,
    )));
    // 6 px total — under TOUCH_SLOP_PX, and enough to move the list a little.
    for y in [148.0, 145.0, 144.0] {
        h.inject(Event::PointerMove(ev(Point::new(x, y), PointerKind::Touch)));
    }
    h.inject(Event::PointerUp(ev(
        Point::new(x, 144.0),
        PointerKind::Touch,
    )));
    h.pump();
    assert_eq!(hits(&h), 1, "a wobbly finger is still a tap");
}

/// Once cancelled, always cancelled: coming back to the press point does not
/// revive the click. (Latched, like every platform's slop.)
#[test]
fn returning_to_the_press_point_does_not_revive_the_click() {
    let mut h = App::new(clickable_rows()).run_headless(Size::new(W, VIEWPORT));
    h.pump();
    let x = 100.0;

    h.inject(Event::PointerDown(ev(
        Point::new(x, 150.0),
        PointerKind::Touch,
    )));
    for y in [110.0, 150.0] {
        h.inject(Event::PointerMove(ev(Point::new(x, y), PointerKind::Touch)));
    }
    h.inject(Event::PointerUp(ev(
        Point::new(x, 150.0),
        PointerKind::Touch,
    )));
    h.pump();
    assert_eq!(hits(&h), 0, "the gesture became a scroll and stayed one");
}

/// Press A, release over B: neither fires. B did not receive the press, and A
/// did not receive the release.
#[test]
fn a_release_on_another_node_activates_neither() {
    let mut h = App::new(two_buttons()).run_headless(Size::new(W, VIEWPORT));
    h.pump();

    h.inject(Event::PointerDown(ev(
        Point::new(40.0, 50.0),
        PointerKind::Mouse,
    )));
    h.inject(Event::PointerUp(ev(
        Point::new(200.0, 50.0),
        PointerKind::Mouse,
    )));
    h.pump();

    let a: Signal<u32> = h.runtime().signal("a", || 0);
    let b: Signal<u32> = h.runtime().signal("b", || 0);
    assert_eq!(a.get(h.runtime()), 0, "A was pressed but not released on");
    assert_eq!(b.get(h.runtime()), 0, "B was released on but not pressed");
}

/// The slop is touch-only. A mouse cannot pan, so there is no gesture to
/// disambiguate — press, drag within the button, release still activates, the
/// way every desktop toolkit behaves.
#[test]
fn a_mouse_drag_inside_one_button_still_clicks() {
    let mut h = App::new(two_buttons()).run_headless(Size::new(W, VIEWPORT));
    h.pump();

    h.inject(Event::PointerDown(ev(
        Point::new(20.0, 20.0),
        PointerKind::Mouse,
    )));
    for x in [40.0, 70.0, 100.0] {
        h.inject(Event::PointerMove(ev(
            Point::new(x, 80.0),
            PointerKind::Mouse,
        )));
    }
    h.inject(Event::PointerUp(ev(
        Point::new(100.0, 80.0),
        PointerKind::Mouse,
    )));
    h.pump();

    let a: Signal<u32> = h.runtime().signal("a", || 0);
    assert_eq!(a.get(h.runtime()), 1, "a mouse wiggle is not a cancel");
}

/// A cancelled release — the platform took the gesture (Android's
/// `MotionAction::Cancel`, which the shell marks with `click_count: 0`) — ends
/// the press without activating anything. Folding cancel into a plain
/// `PointerUp` was harmless while clicks fired on the press; now it would fire
/// one on the way out.
#[test]
fn a_cancelled_release_does_not_click() {
    let mut h = App::new(clickable_rows()).run_headless(Size::new(W, VIEWPORT));
    h.pump();
    let p = Point::new(100.0, 30.0);

    h.inject(Event::PointerDown(ev(p, PointerKind::Touch)));
    let mut up = ev(p, PointerKind::Touch);
    up.click_count = 0;
    h.inject(Event::PointerUp(up));
    h.pump();
    assert_eq!(hits(&h), 0, "a cancelled gesture is not a tap");

    // …and the ordinary release still is, so the gate is on the marker, not on
    // touch releases in general.
    h.inject(Event::PointerDown(ev(p, PointerKind::Touch)));
    h.inject(Event::PointerUp(ev(p, PointerKind::Touch)));
    h.pump();
    assert_eq!(hits(&h), 1);
}
