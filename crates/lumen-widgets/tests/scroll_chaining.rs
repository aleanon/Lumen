//! A scroll container that cannot move must not swallow the wheel.
//!
//! Reported by a downstream app: putting a `VirtualList` inside an outer
//! scroller stole the outer's scroll whenever the inner list was shorter than
//! its viewport (`max_y == 0`), and a tile click then landed on a floating
//! button because the page had not moved. The workaround was structural — do
//! not create an inner scroller unless it can actually scroll — which is a rule
//! no author should have to know.
//!
//! `WheelHandler` returns `()` and so cannot report "not consumed", but
//! `NodeMeta` already carries `ScrollInfo`, so the router can decide without any
//! signature change.

use kurbo::Size;
use lumen_core::events::{Event, WheelEvent};
use lumen_core::geometry::Point;
use lumen_core::state::Signal;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, BuildCx, Element, Scrollable, VirtualList};

/// An outer `Scrollable` (tall content, so it CAN scroll) wrapping a
/// `VirtualList` whose items fit its viewport (so it cannot).
fn nested(inner_items: usize) -> impl Fn(&mut BuildCx) -> Element {
    move |cx: &mut BuildCx| {
        let list = VirtualList::new(cx, "inner", inner_items, 20.0, 100.0, |i| {
            widgets::text(format!("row {i}"))
        });
        let mut list: Element = list.into();
        // A `Scrollable` lays its content out as a flex column, so children
        // SHRINK to fit the viewport unless told not to: without this the 100px
        // list becomes 20px (100 * 200/1000) and the wheel lands on the outer
        // scroller no matter what the router does — the test would pass for the
        // wrong reason.
        list.style.flex_shrink = 0.0;
        let mut filler = widgets::column(vec![]);
        filler.style.height = Dim::px(900.0);
        filler.style.flex_shrink = 0.0;
        let mut scroller: Element = Scrollable::new(cx, "outer", 200.0, 1000.0, vec![list, filler])
            .id("outer")
            .into();
        // A definite width is required, and its absence is not obvious: every
        // item in a `VirtualList` is absolutely positioned, so it contributes no
        // intrinsic width, and a shrink-wrapping ancestor collapses the whole
        // chain to zero — a wheel event then hits nothing at all.
        scroller.style.width = Dim::px(300.0);
        let mut root = widgets::column(vec![scroller]);
        root.style.width = Dim::px(300.0);
        root
    }
}

fn wheel_at(h: &mut lumen_widgets::Headless, x: f64, y: f64, dy: f64) {
    h.inject(Event::Wheel(WheelEvent {
        pos: Point::new(x, y),
        delta: lumen_core::geometry::Vec2 { x: 0.0, y: dy },
        modifiers: Default::default(),
    }));
    h.pump();
}

/// The reported bug: the inner list cannot scroll, so the wheel must reach the
/// outer one.
#[test]
fn a_full_inner_list_does_not_steal_the_outer_scroll() {
    // 3 items x 20px = 60px in a 100px viewport => max_y == 0.
    let mut h = App::new(nested(3)).run_headless(Size::new(300.0, 200.0));
    h.pump();
    wheel_at(&mut h, 150.0, 40.0, 120.0);
    let outer: Signal<f64> = h.runtime().signal("outer", || 0.0);
    assert!(
        outer.get(h.runtime()) > 0.0,
        "the inner list has nothing to scroll, so the wheel must chain to the \
         outer scroller; it stayed at {}",
        outer.get(h.runtime())
    );
}

/// The other half: an inner list that CAN scroll must still consume, or every
/// nested list would scroll its parent instead of itself.
#[test]
fn a_scrollable_inner_list_still_consumes_the_wheel() {
    // 500 items x 20px = 10 000px in a 100px viewport => max_y > 0.
    let mut h = App::new(nested(500)).run_headless(Size::new(300.0, 200.0));
    h.pump();
    wheel_at(&mut h, 150.0, 40.0, 120.0);
    let outer: Signal<f64> = h.runtime().signal("outer", || 0.0);
    let inner: Signal<f64> = h.runtime().signal("inner", || 0.0);
    assert!(
        inner.get(h.runtime()) > 0.0,
        "the inner list can scroll and must take the wheel"
    );
    assert_eq!(
        outer.get(h.runtime()),
        0.0,
        "the outer scroller must not move while the inner one is consuming"
    );
}
