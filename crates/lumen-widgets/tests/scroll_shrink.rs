//! Regression: a `Scrollable` must clamp a stale offset to the CURRENT extent.
//!
//! `Scrollable::new` re-clamps the stored offset on every build, and says why:
//! "content can shrink between builds (a tab switch, a filter), and a stale
//! offset must not push what's left out of the viewport."
//!
//! Nothing tested it. Every existing scroll test uses a `content_h` that is
//! constant for the life of the test, so the clamp is never reached — delete
//! it and they all still pass. The user-visible failure it prevents is a
//! filtered or collapsed list rendering as a BLANK viewport: the content is
//! there, scrolled entirely past its own end, recoverable only by scrolling
//! back up with no indication that is what is needed.

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, BuildCx, Element, Scrollable};

const VIEWPORT: f64 = 100.0;
const ROW_H: f64 = 40.0;
const MANY: usize = 20; // 800 px of content -> max_y = 700
const FEW: usize = 2; //  80 px of content -> max_y =   0

fn row(i: usize) -> Element {
    // Explicit box: a bare text element shrink-wraps to its glyphs, which
    // would make the geometry assertions below depend on font metrics.
    let mut e = widgets::text(format!("row {i}")).id(format!("row-{i}"));
    e.style.height = Dim::px(ROW_H as f32);
    e.style.width = Dim::pct(1.0);
    e
}

fn app() -> App {
    App::new(|cx: &mut BuildCx| {
        let n = cx.signal("rows", || MANY).get(cx.runtime());
        let rows: Vec<Element> = (0..n).map(row).collect();
        Scrollable::new(cx, "sc", VIEWPORT, (n as f64) * ROW_H, rows).into()
    })
}

#[test]
fn shrinking_the_content_under_a_scrolled_offset_still_shows_it() {
    let mut h = app().run_headless(Size::new(200.0, VIEWPORT));
    h.pump();

    // Scroll to the very bottom of the tall list.
    let off: Signal<f64> = h.runtime().signal("sc", || 0.0);
    off.set(h.runtime(), (MANY as f64) * ROW_H - VIEWPORT); // 700
    h.pump();

    let last = h
        .node_bounds_by_id(&format!("row-{}", MANY - 1))
        .expect("last row laid out");
    assert!(
        last.y1 > 0.0 && last.y0 < VIEWPORT,
        "precondition: scrolled to the bottom, the last row must be in the \
         viewport, got {last:?}"
    );

    // Now the content shrinks under the stored offset — a filter, a collapse,
    // a tab switch. max_y becomes 0, so the offset must be clamped to 0.
    let n: Signal<usize> = h.runtime().signal("rows", || MANY);
    n.set(h.runtime(), FEW);
    h.pump();

    let first = h.node_bounds_by_id("row-0").expect("first row laid out");
    assert!(
        first.y0 >= -0.5 && first.y0 < VIEWPORT,
        "the stale 700px offset was not clamped: row-0 is at y0={} and the \
         viewport is 0..{VIEWPORT}, i.e. the list renders blank",
        first.y0
    );
    // …and the whole remaining content fits, so nothing is pushed out.
    let last_now = h
        .node_bounds_by_id(&format!("row-{}", FEW - 1))
        .expect("last remaining row laid out");
    assert!(
        last_now.y1 <= VIEWPORT + 0.5,
        "remaining content should fit the viewport once clamped, got y1={}",
        last_now.y1
    );
}

/// The clamp is applied to the RENDER, not written back to the signal.
///
/// This is the property that distinguishes the real implementation from the
/// obvious wrong fix (clamping the stored value), and it is only observable by
/// shrinking and then growing again: if the shrink had written 0 back, the
/// offset would be lost and the list would jump to the top when the content
/// returns.
///
/// The first version of this test set the offset AFTER the shrink, at a value
/// the final extent made legal — so `clamp` was an identity function on it and
/// the test passed with the clamp deleted. It asserted nothing.
#[test]
fn the_clamp_is_render_only_and_does_not_destroy_the_stored_offset() {
    let mut h = app().run_headless(Size::new(200.0, VIEWPORT));
    h.pump();

    let off: Signal<f64> = h.runtime().signal("sc", || 0.0);
    let bottom = (MANY as f64) * ROW_H - VIEWPORT; // 700
    off.set(h.runtime(), bottom);
    h.pump();

    // Shrink: the render must clamp to 0…
    let n: Signal<usize> = h.runtime().signal("rows", || MANY);
    n.set(h.runtime(), FEW);
    h.pump();
    let first = h.node_bounds_by_id("row-0").expect("row-0 laid out");
    assert!(
        first.y0.abs() < 0.5,
        "while shrunk, the render must clamp to the top; got y0={}",
        first.y0
    );

    // …but the stored 700 must survive, so growing back restores the position.
    n.set(h.runtime(), MANY);
    h.pump();
    let first = h.node_bounds_by_id("row-0").expect("row-0 laid out");
    assert!(
        (first.y0 + bottom).abs() < 0.5,
        "the offset must be clamped for display only, never written back: \
         after growing again row-0 should be at y0={}, got {}",
        -bottom,
        first.y0
    );
}
