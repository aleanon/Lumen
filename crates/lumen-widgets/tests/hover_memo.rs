//! A.1 (docs/plan-retained-pipeline.md): hover/focus/pressed changes rebuild
//! the frame but must NOT clear the `cx.scope` memo caches — no view closure
//! can observe visual state (`BuildCx` exposes no accessor), so memoized
//! subtrees cannot be stale. Guards pointer motion staying F1-memoized.

use std::cell::Cell;
use std::rc::Rc;

use kurbo::{Point, Size};
use lumen_core::events::{Event, PointerEvent};
use lumen_widgets::{center, col, widgets, App};

fn bg_of(styles: &serde_json::Value) -> Option<String> {
    styles
        .get("background")?
        .get("value")?
        .as_str()
        .map(str::to_string)
}

#[test]
fn hover_change_reuses_memoized_scopes() {
    let runs = Rc::new(Cell::new(0u32));
    let runs_outer = runs.clone();
    let mut h = App::new(move |cx| {
        let runs = runs_outer.clone();
        col![
            cx.scope("expensive", move |_cx| {
                runs.set(runs.get() + 1);
                widgets::text("memoized subtree")
            }),
            widgets::button("Hover me", |_| {}).id("b"),
        ]
    })
    .stylesheet(
        "button { background: #00ff00ff; } \
         button:hovered { background: #ff0000ff; }",
    )
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    assert_eq!(bg_of(&h.get_styles("#b")).as_deref(), Some("#00ff00ff"));
    let baseline = runs.get();
    assert!(baseline >= 1, "scope ran on the first build");

    // Pointer moves onto the button: the `:hovered` styling applies through
    // a rebuild…
    let p = center(h.node_bounds_by_id("b").expect("button laid out"));
    h.inject(Event::PointerMove(PointerEvent::at(p)));
    h.pump();
    assert_eq!(bg_of(&h.get_styles("#b")).as_deref(), Some("#ff0000ff"));
    // …without re-running the memoized scope.
    assert_eq!(runs.get(), baseline, "hover wiped the scope caches");

    // And back off again.
    h.inject(Event::PointerMove(PointerEvent::at(Point::new(2.0, 2.0))));
    h.pump();
    assert_eq!(bg_of(&h.get_styles("#b")).as_deref(), Some("#00ff00ff"));
    assert_eq!(runs.get(), baseline, "unhover wiped the scope caches");

    // The incremental view still matches a from-scratch rebuild. (Runs after
    // the counter asserts — the oracle re-runs closures by design.)
    h.assert_view_coherent();
}
