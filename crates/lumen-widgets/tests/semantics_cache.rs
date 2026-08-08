//! OB4: `semantics_elided()` memoizes the elided tree. A memo is only safe if
//! it is invalidated everywhere `sem_root` is reassigned — a stale entry would
//! hand the agent, the test harness, and assistive tech a tree that disagrees
//! with what is on screen, which is precisely the failure the framework's
//! same-tree design exists to make impossible.
//!
//! There are two assignment sites (the full rebuild and the hover/restyle
//! path). Both are exercised here.

use kurbo::{Point, Size};
use lumen_core::events::{Event, PointerEvent};
use lumen_widgets::{col, widgets, App, BuildCx, Element};

fn find<'a>(
    n: &'a lumen_core::semantics::SemanticsNode,
    id: &str,
) -> Option<&'a lumen_core::semantics::SemanticsNode> {
    if n.id.as_ref().is_some_and(|s| s.as_str() == id) {
        return Some(n);
    }
    n.children.iter().find_map(|c| find(c, id))
}

fn app() -> App {
    App::new(|cx: &mut BuildCx| -> Element {
        let n = cx.signal("count", || 0i64);
        col![
            widgets::text(format!("value: {}", n.get(cx.runtime()))).id("label"),
            widgets::button("bump", move |rt| n.update(rt, |v| *v += 1)).id("bump")
        ]
    })
}

#[test]
fn rebuild_invalidates_the_elided_cache() {
    let mut h = app().run_headless(Size::new(300.0, 200.0));
    h.pump();

    let before = h.semantics_elided();
    let label = find(&before, "label").expect("label present");
    assert_eq!(label.label, "value: 0", "initial render");

    // Warm the cache deliberately, so a missing invalidation would be caught
    // rather than masked by the first call happening after the rebuild.
    let _warm = h.semantics_elided();

    let p = {
        let b = h.node_bounds_by_id("bump").expect("bump has bounds");
        Point::new(b.x0 + b.width() / 2.0, b.y0 + b.height() / 2.0)
    };
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();

    let after = h.semantics_elided();
    let label = find(&after, "label").expect("label still present");
    assert_eq!(
        label.label, "value: 1",
        "cache must not survive a rebuild — a stale tree would disagree with \
         the rendered frame"
    );
}

#[test]
fn restyle_invalidates_the_elided_cache() {
    let mut h = app().run_headless(Size::new(300.0, 200.0));
    h.pump();

    let b = h.node_bounds_by_id("bump").expect("bump has bounds");
    let inside = Point::new(b.x0 + b.width() / 2.0, b.y0 + b.height() / 2.0);
    let outside = Point::new(b.x0 - 5.0, b.y0 - 5.0);

    // Ensure we start un-hovered, and warm the cache in that state.
    h.inject(Event::PointerMove(PointerEvent::at(outside)));
    h.pump();
    let cold = h.semantics_elided();
    let hovered_before = find(&cold, "bump")
        .map(|n| {
            n.states
                .contains(&lumen_core::semantics::State::Hovered)
        })
        .unwrap_or(false);
    assert!(!hovered_before, "precondition: not hovered");

    // Hover takes the restyle path, which reassigns sem_root without a full
    // rebuild — the site most likely to be forgotten when adding a cache.
    h.inject(Event::PointerMove(PointerEvent::at(inside)));
    h.pump();

    let hot = h.semantics_elided();
    let hovered_after = find(&hot, "bump")
        .map(|n| {
            n.states
                .contains(&lumen_core::semantics::State::Hovered)
        })
        .unwrap_or(false);
    assert!(
        hovered_after,
        "hover state must be visible after the restyle path — a stale cache \
         here would hide every hover/focus/press transition from the agent"
    );
}

#[test]
fn repeated_calls_share_one_projection() {
    let mut h = app().run_headless(Size::new(300.0, 200.0));
    h.pump();

    // The point of the cache: two calls with no intervening pump must not
    // rebuild the tree. Sharing is observable through the Rc's strong count.
    let a = h.semantics_elided();
    let b = h.semantics_elided();
    assert!(
        std::rc::Rc::ptr_eq(&a, &b),
        "repeated calls should share one projection, not re-elide"
    );
}
