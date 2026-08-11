//! A finger drag scrolls a list; a mouse drag does not.
//!
//! Before this, touch could not scroll at all. The Android shell translates
//! `MotionEvent` into `PointerDown`/`Move`/`Up` and never emits a wheel, while
//! `Scrollable`/`VirtualList` only handle `on_wheel` and `on_key` — so on a
//! phone a list could be moved only by dragging its scrollbar.
//!
//! Restricted to `PointerKind::Touch` on purpose: a mouse drag inside a list is
//! a text selection or a marquee, and turning it into a scroll would break both.

use kurbo::Size;
use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
use lumen_core::geometry::Point;
use lumen_core::state::Signal;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, BuildCx, Element, VirtualList};

const W: f64 = 300.0;
const VIEWPORT: f64 = 200.0;

fn list() -> impl Fn(&mut BuildCx) -> Element {
    |cx: &mut BuildCx| {
        let vl = VirtualList::new(cx, "vl", 1000, 20.0, VIEWPORT, |i| {
            widgets::text(format!("row {i}"))
        });
        let mut root = widgets::column(vec![vl.into()]);
        root.style.width = Dim::px(W as f32);
        root.id("root")
    }
}

fn ev(pos: Point, kind: PointerKind) -> PointerEvent {
    PointerEvent {
        pos,
        button: PointerButton::Left,
        pointer: kind,
        modifiers: Default::default(),
        click_count: 1,
    }
}

/// Drag a finger up the middle of the list: content follows, so it scrolls
/// toward the end.
#[test]
fn a_finger_drag_scrolls_the_list() {
    let mut h = App::new(list()).run_headless(Size::new(W, VIEWPORT));
    h.pump();
    // Well left of the scrollbar track, so this is a content drag.
    let x = 100.0;
    h.inject(Event::PointerDown(ev(
        Point::new(x, 150.0),
        PointerKind::Touch,
    )));
    for y in [130.0, 110.0, 90.0, 70.0] {
        h.inject(Event::PointerMove(ev(Point::new(x, y), PointerKind::Touch)));
    }
    h.inject(Event::PointerUp(ev(
        Point::new(x, 70.0),
        PointerKind::Touch,
    )));
    h.pump();
    let y: Signal<f64> = h.runtime().signal("vl", || 0.0);
    assert_eq!(
        y.get(h.runtime()),
        80.0,
        "dragging a finger 80px up should scroll 80px toward the end"
    );
}

/// The same gesture with a mouse must NOT scroll — that is a selection drag.
#[test]
fn a_mouse_drag_does_not_scroll_the_list() {
    let mut h = App::new(list()).run_headless(Size::new(W, VIEWPORT));
    h.pump();
    let x = 100.0;
    h.inject(Event::PointerDown(ev(
        Point::new(x, 150.0),
        PointerKind::Mouse,
    )));
    for y in [130.0, 110.0, 90.0, 70.0] {
        h.inject(Event::PointerMove(ev(Point::new(x, y), PointerKind::Mouse)));
    }
    h.inject(Event::PointerUp(ev(
        Point::new(x, 70.0),
        PointerKind::Mouse,
    )));
    h.pump();
    let y: Signal<f64> = h.runtime().signal("vl", || 0.0);
    assert_eq!(y.get(h.runtime()), 0.0, "a mouse drag must not pan");
}

/// Panning chains like a wheel: a finger on an inner list that cannot scroll
/// moves the outer one.
#[test]
fn a_finger_drag_chains_past_a_list_that_cannot_scroll() {
    let build = |cx: &mut BuildCx| -> Element {
        // 3 rows in a 100px viewport => the inner list cannot scroll.
        let inner = VirtualList::new(cx, "inner", 3, 20.0, 100.0, |i| {
            widgets::text(format!("row {i}"))
        });
        let mut inner: Element = inner.into();
        inner.style.flex_shrink = 0.0;
        let mut filler = widgets::column(vec![]);
        filler.style.height = Dim::px(900.0);
        filler.style.flex_shrink = 0.0;
        let mut sc: Element =
            lumen_widgets::Scrollable::new(cx, "outer", 200.0, 1000.0, vec![inner, filler])
                .id("outer")
                .into();
        sc.style.width = Dim::px(W as f32);
        let mut root = widgets::column(vec![sc]);
        root.style.width = Dim::px(W as f32);
        root.id("root")
    };
    let mut h = App::new(build).run_headless(Size::new(W, VIEWPORT));
    h.pump();
    let x = 100.0;
    h.inject(Event::PointerDown(ev(
        Point::new(x, 60.0),
        PointerKind::Touch,
    )));
    h.inject(Event::PointerMove(ev(
        Point::new(x, 20.0),
        PointerKind::Touch,
    )));
    h.pump();
    let outer: Signal<f64> = h.runtime().signal("outer", || 0.0);
    assert_eq!(
        outer.get(h.runtime()),
        40.0,
        "the inner list has nowhere to go, so the finger must move the outer one"
    );
}
