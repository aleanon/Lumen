//! A.3.2 (docs/plan-retained-pipeline.md): memo-hit scopes stop deep-cloning —
//! a cache hit hands `build_node` an `Rc` stub, and when sound the scope's
//! span is **copied forward** from the previous build (meta/styles/layout
//! styles moved across, flags refreshed) instead of re-lowered. The
//! `FrameStats::{nodes_rebuilt, nodes_copied}` meters make O(changed)
//! observable; the coherence oracle guards equality with rebuild-fresh.

use kurbo::Size;
use lumen_core::events::{Event, PointerEvent};
use lumen_core::state::Signal;
use lumen_widgets::{center, col, widgets, App};

#[test]
fn signal_write_copies_untouched_scope_forward() {
    let mut h = App::new(|cx| {
        let a = cx.scope("a", |_cx| {
            col![
                widgets::text("static-one"),
                widgets::text("static-two"),
                widgets::text("static-three")
            ]
        });
        let b = cx.scope("b", |cx| {
            let n: Signal<i64> = cx.signal("n", || 0);
            col![widgets::text(format!("n={}", n.get(cx.runtime()))).id("out")]
        });
        col![a, b]
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();

    // `cx.signal` inside scope "b" namespaces the key.
    let n: Signal<i64> = h.runtime().signal("b/n", || 0);
    n.set(h.runtime(), 7);
    let stats = h.pump();

    assert!(h.semantics_json().to_string().contains("n=7"));
    assert!(
        stats.nodes_copied >= 4,
        "scope a (col + 3 texts) copied forward: {stats:?}"
    );
    assert!(
        stats.nodes_rebuilt < stats.nodes_copied + stats.nodes_rebuilt,
        "some nodes copied: {stats:?}"
    );
    // O(changed): the fresh-lowered set excludes scope a's subtree.
    assert!(
        stats.nodes_rebuilt <= 4,
        "only root chrome + scope b re-lowered: {stats:?}"
    );
    h.assert_view_coherent();
}

#[test]
fn visual_state_rebuilds_never_copy_forward() {
    let mut h = App::new(|cx| {
        col![
            cx.scope("s", |_cx| widgets::text("cached")),
            widgets::button("Hover", |_| {}).id("b")
        ]
    })
    .stylesheet("button:hovered { background: #ff0000ff; }")
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    h.pump(); // second build: scope cached

    let p = center(h.node_bounds_by_id("b").unwrap());
    h.inject(Event::PointerMove(PointerEvent::at(p)));
    let stats = h.pump();
    assert_eq!(
        stats.nodes_copied, 0,
        "hover rebuild must re-resolve state parts everywhere: {stats:?}"
    );
    h.assert_view_coherent();
}

#[test]
fn ancestor_class_change_invalidates_the_copy() {
    // The cached scope's button is styled through a *descendant* combinator on
    // an ancestor class the scope itself never reads. Flipping the class must
    // re-resolve the scope's styles (context-hash mismatch → full lower).
    let mut h = App::new(|cx| {
        let warn: Signal<bool> = cx.signal("warn", || false);
        let is_warn = warn.get(cx.runtime());
        let inner = cx.scope("inner", |_cx| {
            col![widgets::button("Save", |_| {}).id("save")]
        });
        let mut wrap = col![inner];
        if is_warn {
            wrap = wrap.class("warn");
        }
        col![wrap]
    })
    .stylesheet(".warn button { background: #ff0000ff; }")
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    h.pump(); // cache warm

    let warn: Signal<bool> = h.runtime().signal("warn", || false);
    warn.set(h.runtime(), true);
    h.pump();
    let bg = h.get_styles("#save")["background"]["value"]
        .as_str()
        .map(str::to_string);
    assert_eq!(
        bg.as_deref(),
        Some("#ff0000ff"),
        "descendant-combinator style from the new ancestor class applies"
    );
    h.assert_view_coherent();
}

#[test]
fn pointer_motion_still_reuses_memoized_scopes() {
    // A.1's guarantee survives A.3.2: hover rebuilds skip closure re-runs
    // even though they re-lower (no copy-forward).
    use std::cell::Cell;
    use std::rc::Rc;
    let runs = Rc::new(Cell::new(0u32));
    let runs_outer = runs.clone();
    let mut h = App::new(move |cx| {
        let runs = runs_outer.clone();
        col![
            cx.scope("exp", move |_cx| {
                runs.set(runs.get() + 1);
                widgets::text("memo")
            }),
            widgets::button("Hover", |_| {}).id("b")
        ]
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    let baseline = runs.get();

    let p = center(h.node_bounds_by_id("b").unwrap());
    h.inject(Event::PointerMove(PointerEvent::at(p)));
    h.pump();
    assert_eq!(runs.get(), baseline, "hover re-ran a memoized scope");
    h.assert_view_coherent();
}
