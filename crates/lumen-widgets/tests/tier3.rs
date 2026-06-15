//! T2.4 acceptance: a tier-3 snapshot restart (kill → rebuild → restore)
//! round-trips signals, scroll offset, and focus.

use kurbo::{Point, Size, Vec2};
use lumen_core::events::{Event, Modifiers, PointerEvent, WheelEvent};
use lumen_core::semantics::SemanticsNode;
use lumen_widgets::{widgets, App, BuildCx, Element, Headless};

/// Counter button + a scrollable list — exercises a signal, focus, and a scroll
/// offset (itself a signal) in one tree.
fn build(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i32);
    let v = count.get(cx.runtime());
    let lines: Vec<Element> = (0..10)
        .map(|i| widgets::text(format!("line {i}")))
        .collect();
    widgets::column(vec![
        widgets::text(format!("Count: {v}")).id("count"),
        widgets::button("+1", move |rt| count.update(rt, |c| *c += 1)).id("inc"),
        widgets::scroll(cx, "sc", 100.0, 300.0, lines).id("sc"),
    ])
}

fn sem(h: &Headless) -> SemanticsNode {
    h.semantics_doc().root.elided()
}

fn by_id<'a>(n: &'a SemanticsNode, id: &str) -> Option<&'a SemanticsNode> {
    if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
        return Some(n);
    }
    n.children.iter().find_map(|c| by_id(c, id))
}

fn mid(n: &SemanticsNode) -> Point {
    let b = n.bounds;
    Point::new(b.x0 + b.width() / 2.0, b.y0 + b.height() / 2.0)
}

#[test]
fn tier3_restart_preserves_signals_scroll_and_focus() {
    let size = Size::new(220.0, 260.0);
    let mut h = App::new(build).run_headless(size);

    // Drive the app into a non-default state: count = 5, focus the button,
    // scroll the list down by 40px.
    let inc = mid(by_id(&sem(&h), "inc").unwrap());
    for _ in 0..5 {
        h.inject(Event::PointerDown(PointerEvent::at(inc)));
        h.inject(Event::PointerUp(PointerEvent::at(inc)));
        h.pump();
    }
    let sc = mid(by_id(&sem(&h), "sc").unwrap());
    h.inject(Event::Wheel(WheelEvent {
        pos: sc,
        delta: Vec2::new(0.0, 40.0),
        modifiers: Modifiers::empty(),
    }));
    h.pump();

    // Preconditions hold on the live instance.
    assert_eq!(by_id(&sem(&h), "count").unwrap().label, "Count: 5");
    assert_eq!(
        by_id(&sem(&h), "sc").unwrap().scroll.map(|s| s.y),
        Some(40.0)
    );

    // Snapshot, serialize to bytes (the artifact written before the kill).
    let snap = h.snapshot();
    let wire = serde_json::to_string(&snap).expect("serialize snapshot");
    let before = serde_json::to_value(snap).unwrap();
    drop(h); // "kill" the process/instance

    // Rebuild a fresh instance and restore from the deserialized snapshot.
    let restored = serde_json::from_str(&wire).expect("deserialize snapshot");
    let (h2, diags) = App::new(build).run_headless_restored(size, restored);

    assert!(diags.is_empty(), "no state dropped on restore: {diags:?}");

    // The reactive store + focus round-trip exactly.
    let after = serde_json::to_value(h2.snapshot()).unwrap();
    assert_eq!(
        before, after,
        "snapshot must survive kill/restore unchanged"
    );

    // …and the restored state actually drives the live UI.
    assert_eq!(by_id(&sem(&h2), "count").unwrap().label, "Count: 5");
    assert_eq!(
        by_id(&sem(&h2), "sc").unwrap().scroll.map(|s| s.y),
        Some(40.0),
        "scroll offset restored"
    );
}

#[test]
fn fresh_start_without_snapshot_is_default() {
    // Guards the test above: without a restore the same app boots at defaults,
    // proving it was the snapshot that carried the state.
    let size = Size::new(220.0, 260.0);
    let h = App::new(build).run_headless(size);
    assert_eq!(by_id(&sem(&h), "count").unwrap().label, "Count: 0");
    assert_eq!(
        by_id(&sem(&h), "sc").unwrap().scroll.map(|s| s.y),
        Some(0.0)
    );
}
