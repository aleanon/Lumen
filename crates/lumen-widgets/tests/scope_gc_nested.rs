//! F5 GC × F1 memoization: a scope that is still on screen must keep its state
//! even when its *parent* was memo-skipped that build.
//!
//! `sweep_dead_scopes` decides liveness from `scope_live`, which `cx.scope`
//! populates as it is called. A memo hit on the parent means the child's
//! `cx.scope` call never runs — so without transitive marking the child looks
//! absent and gets swept while it is still very much in the view.

use std::cell::Cell;
use std::rc::Rc;

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App, BuildCx};

/// outer "o" (reads `odep`) wrapping inner "i" (owns a local signal), beside
/// sibling "s" (reads `sib`). `inits` counts how often the inner scope's local
/// signal ran its *initializer* — i.e. how often its state was lost.
fn nested(inits: Rc<Cell<u32>>) -> App {
    App::new(move |cx: &mut BuildCx| {
        let odep: Signal<i64> = cx.signal("odep", || 0);
        let sib: Signal<i64> = cx.signal("sib", || 0);
        let inits = inits.clone();
        widgets::column(vec![
            cx.scope("o", move |cx| {
                let n = odep.get(cx.runtime());
                let inner = cx.scope("i", |cx| {
                    let local: Signal<i64> = cx.signal("local", || {
                        inits.set(inits.get() + 1);
                        0
                    });
                    widgets::text(format!("local={}", local.get(cx.runtime()))).id("i")
                });
                widgets::column(vec![widgets::text(format!("outer={n}")), inner])
            }),
            cx.scope("s", move |cx| {
                widgets::text(format!("sib={}", sib.get(cx.runtime()))).id("s")
            }),
        ])
    })
}

#[test]
fn memo_skipped_parent_does_not_orphan_its_child_scope() {
    let inits = Rc::new(Cell::new(0));
    let mut h = nested(inits.clone()).run_headless(Size::new(200.0, 160.0));
    assert_eq!(inits.get(), 1, "inner initialized once on the first build");

    let odep: Signal<i64> = h.runtime().signal("odep", || 0);
    let sib: Signal<i64> = h.runtime().signal("sib", || 0);

    // Touch only the sibling: "o" keeps its cached subtree, so "i" is never
    // re-entered this build and is absent from `scope_live`.
    sib.set(h.runtime(), 1);
    h.pump();

    // Now force "o" to re-run. If the sweep evicted "i" above, its local signal
    // is recreated from scratch and the initializer runs a second time.
    odep.set(h.runtime(), 1);
    h.pump();
    assert_eq!(
        inits.get(),
        1,
        "inner scope's state was swept while its parent was merely memo-skipped"
    );
}
