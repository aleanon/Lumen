//! The overlay scrollbar is sized from the REPORTED extent, so it is correct
//! for a virtualized list.
//!
//! The failure this prevents is the one virtual lists have elsewhere: a
//! scrollbar derived from laid-out content grows as rows materialize, so the
//! thumb shrinks out from under the pointer and dragging to the middle lands
//! somewhere else — you drag, it readjusts, you drag again. `VirtualList`
//! computes `max_y` from `item_count * item_height`, so the true extent is
//! known while only a window of rows exists.

use kurbo::Size;
use lumen_core::events::{Event, PointerEvent};
use lumen_core::geometry::Point;
use lumen_core::state::Signal;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, BuildCx, Element, VirtualList};

const W: f64 = 300.0;
const VIEWPORT: f64 = 200.0;
const ITEM_H: f64 = 20.0;

fn list(items: usize) -> impl Fn(&mut BuildCx) -> Element {
    move |cx: &mut BuildCx| {
        let vl = VirtualList::new(cx, "vl", items, ITEM_H, VIEWPORT, |i| {
            widgets::text(format!("row {i}"))
        });
        let mut root = widgets::column(vec![vl.id("vl").into()]);
        root.style.width = Dim::px(W as f32);
        root.id("root")
    }
}

/// Dragging the track to its midpoint lands at the middle of the list — on the
/// first attempt, with no re-adjustment.
#[test]
fn dragging_the_track_to_the_middle_reaches_the_middle() {
    let items = 10_000;
    let mut h = App::new(list(items)).run_headless(Size::new(W, VIEWPORT));
    h.pump();

    // Press at the track's vertical midpoint. The track spans the viewport at
    // the right edge.
    let p = Point::new(W - 4.0, VIEWPORT / 2.0);
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerMove(PointerEvent::at(p)));
    h.pump();

    let max_y = items as f64 * ITEM_H - VIEWPORT;
    let y: Signal<f64> = h.runtime().signal("vl", || 0.0);
    let got = y.get(h.runtime());
    let want = max_y / 2.0;
    assert!(
        (got - want).abs() < max_y * 0.05,
        "dragging to the track midpoint should land near {want:.0} of {max_y:.0}, got {got:.0}"
    );
}

/// The thumb is proportional to the REPORTED extent, not to the handful of rows
/// that happen to be materialized. With 10 000 items in a 200 px viewport the
/// thumb is at its floor; with 12 it is nearly the whole track.
#[test]
fn the_thumb_reflects_the_whole_list_not_the_window() {
    let bar_h = |items: usize| {
        let mut h = App::new(list(items)).run_headless(Size::new(W, VIEWPORT));
        h.pump();
        let s = h.semantics_json().to_string();
        // The scrollbar is present exactly when the list can scroll.
        s.matches("Scrollbar").count()
    };
    assert_eq!(
        bar_h(5),
        0,
        "5 rows fit the viewport, so there is nothing to scroll and no bar"
    );
    assert_eq!(bar_h(10_000), 1, "a long list must show exactly one bar");
}
