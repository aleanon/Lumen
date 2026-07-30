//! Regression: hit-testing must respect a clipping viewport.
//!
//! `Scrollable` scrolls by giving its content a negative top margin and masking
//! the overflow with `clip: true`. That clip is applied to *painting*; this test
//! guards that it is also applied to *hit-testing*, so a row scrolled above the
//! viewport (laid out at negative Y, painted-clipped) cannot keep a live hitbox
//! outside the box and steal a click from a widget sitting above the list.
//!
//! Before the fix, `Tree`'s per-node clip rects were never populated in
//! production (`set_clip` was only ever called from unit tests), so the clip
//! check in `hit_test` was a permanent no-op.

use kurbo::{Point, Size};
use lumen_core::events::{Event, PointerEvent};
use lumen_core::state::Signal;
use lumen_widgets::{center, col, widgets, App, BuildCx, Element, Scrollable};

fn click_at(h: &mut lumen_widgets::Headless, p: Point) {
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
}

#[test]
fn scroll_viewport_clips_hit_testing() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let hit = cx.signal("hit", String::new);
        // A button directly above the list.
        let button: Element =
            widgets::button("above", move |rt| hit.set(rt, "button".to_string())).id("above");
        // A tall list of clickable rows inside a short (90px) viewport.
        let rows: Vec<Element> = (0..20)
            .map(|i| {
                widgets::button(format!("row {i}"), move |rt| {
                    hit.set(rt, format!("row-{i}"))
                })
                .id(format!("row-{i}"))
            })
            .collect();
        let list: Element = Scrollable::new(cx, "sc", 90.0, 600.0, rows).into();
        col![button, list]
    })
    .run_headless(Size::new(220.0, 400.0));
    h.pump();

    let bb = h.node_bounds_by_id("above").expect("button laid out");
    let click = center(bb);
    let hit: Signal<String> = h.runtime().signal("hit", String::new);

    // Sanity: the button is clickable before any scrolling.
    click_at(&mut h, click);
    assert_eq!(
        hit.get(h.runtime()),
        "button",
        "button should be clickable before scrolling"
    );
    hit.set(h.runtime(), String::new());

    // Scroll the list down so the top rows are laid out ABOVE the viewport.
    let off: Signal<f64> = h.runtime().signal("sc", || 0.0);
    off.set(h.runtime(), 200.0);
    h.pump();

    // Precondition: a scrolled-out row now overlaps the button's point. Without
    // the hit-test clip that row would win the click (later in document order),
    // so this makes the test a genuine regression rather than a silent no-op.
    let overlapping = (0..20).any(|i| {
        h.node_bounds_by_id(&format!("row-{i}"))
            .is_some_and(|b| b.contains(click))
    });
    assert!(
        overlapping,
        "precondition: a scrolled-out row must overlap the button's point"
    );

    // The click must STILL land on the button — the viewport's clip rejects the
    // scrolled-out row's hitbox because it falls outside the box.
    click_at(&mut h, click);
    assert_eq!(
        hit.get(h.runtime()),
        "button",
        "a scrolled-out row escaped the scroll viewport's clip and stole the click"
    );
}
