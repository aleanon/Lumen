//! `cx.scope_with_deps` — memoizing a subtree that is a function of plain data.
//!
//! `cx.scope` invalidates on the signals its closure *reads*. The dangerous
//! shape is a closure that reads none, because the data was read by its parent
//! and captured:
//!
//! ```ignore
//! let items = items.get(rt);                      // read HERE
//! cx.scope(("row", i), move |_| row(&items[i]))   // reads nothing
//! ```
//!
//! An empty `ReadSet` is always "current", so that scope is memo-hit forever
//! and the row freezes. `first_shows_the_hazard` below is that bug, asserted as
//! present, so nobody "fixes" `scope` into unsoundness later; the rest prove
//! `scope_with_deps` is the way out.

use lumen_core::geometry::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App, BuildCx, Element};
use std::cell::Cell;
use std::rc::Rc;

/// Label of the node with `id`, from the semantics tree.
fn label(h: &lumen_widgets::Headless, id: &str) -> String {
    fn find(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<String> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.label.clone());
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    find(&h.semantics_doc().root, id).unwrap_or_default()
}

/// A plain `cx.scope` around a closure that reads no signal memo-hits forever.
///
/// Asserted deliberately: this is why `VirtualList` does not memoize by
/// default, and why `scope_with_deps` exists. If this test ever starts failing,
/// `scope`'s invalidation changed and the widgets' opt-in story should be
/// revisited — not this assertion deleted.
#[test]
fn first_shows_the_hazard_plain_scope_freezes() {
    let build = |cx: &mut BuildCx| -> Element {
        // Read in the PARENT: the row scope below reads nothing.
        let v = cx.signal("v", || 0i64).get(cx.runtime());
        let row = cx.scope("row", move |_cx: &mut BuildCx| {
            widgets::text(format!("v={v}")).id("row")
        });
        widgets::column(vec![row]).id("root")
    };
    let mut h = App::new(build).run_headless(Size::new(200.0, 80.0));
    h.pump();
    assert_eq!(label(&h, "row"), "v=0");

    let v: Signal<i64> = h.runtime().signal("v", || 0);
    v.set(h.runtime(), 7);
    h.pump();
    assert_eq!(
        label(&h, "row"),
        "v=0",
        "if this now reads v=7, plain `cx.scope` learned to see captured data — \
         good news, but the widgets' opt-in memoization should be re-examined"
    );
}

/// The same shape with `scope_with_deps` tracks the captured value.
#[test]
fn deps_invalidate_a_scope_that_reads_nothing() {
    let build = |cx: &mut BuildCx| -> Element {
        let v = cx.signal("v", || 0i64).get(cx.runtime());
        let row = cx.scope_with_deps("row", v, move |_cx: &mut BuildCx| {
            widgets::text(format!("v={v}")).id("row")
        });
        widgets::column(vec![row]).id("root")
    };
    let mut h = App::new(build).run_headless(Size::new(200.0, 80.0));
    h.pump();
    assert_eq!(label(&h, "row"), "v=0");

    let v: Signal<i64> = h.runtime().signal("v", || 0);
    v.set(h.runtime(), 7);
    h.pump();
    assert_eq!(
        label(&h, "row"),
        "v=7",
        "a changed dep must re-run the closure"
    );
}

/// …and still memoizes when the deps hold. Counted, because a memo that never
/// hits is indistinguishable from correctness while being worthless — the
/// ADR-021 trap that silently no-op'd `scope_memo_one_of_many`.
#[test]
fn unchanged_deps_skip_the_closure() {
    let runs = Rc::new(Cell::new(0u32));
    let r = runs.clone();
    let build = move |cx: &mut BuildCx| -> Element {
        let tick = cx.signal("tick", || 0i64).get(cx.runtime());
        let dep = 1i64; // never changes
        let r = r.clone();
        let row = cx.scope_with_deps("row", dep, move |_cx: &mut BuildCx| {
            r.set(r.get() + 1);
            widgets::text("stable").id("row")
        });
        widgets::column(vec![row, widgets::text(format!("{tick}")).id("tick")]).id("root")
    };
    let mut h = App::new(build).run_headless(Size::new(200.0, 80.0));
    h.pump();
    let after_first = runs.get();
    assert_eq!(after_first, 1, "the closure runs once to begin with");

    let tick: Signal<i64> = h.runtime().signal("tick", || 0);
    for i in 1..=5 {
        tick.set(h.runtime(), i);
        h.pump();
    }
    assert_eq!(
        runs.get(),
        after_first,
        "five rebuilds with unchanged deps must not re-run the memoized closure"
    );
    assert_eq!(
        label(&h, "tick"),
        "5",
        "…while the rest of the tree did rebuild"
    );
}

/// Identity survives a deps change: scope-local state is not shed.
///
/// This is the reason deps live beside the key rather than folded into it.
/// Folding would make a changed dep a *different* scope, silently resetting any
/// signal the row owns.
#[test]
fn a_deps_change_keeps_scope_local_state() {
    let build = |cx: &mut BuildCx| -> Element {
        let v = cx.signal("v", || 0i64).get(cx.runtime());
        let row = cx.scope_with_deps("row", v, move |cx: &mut BuildCx| {
            // Scope-local: identified relative to this scope.
            let local: Signal<i64> = cx.signal("local", || 0);
            widgets::text(format!("v={v} local={}", local.get(cx.runtime()))).id("row")
        });
        widgets::column(vec![row]).id("root")
    };
    let mut h = App::new(build).run_headless(Size::new(240.0, 80.0));
    h.pump();

    // Set the row's own state through the scope's identity path.
    let scope = lumen_core::identity::ScopePath::root().child("row");
    let local: Signal<i64> = h.runtime().signal_at(
        lumen_core::identity::fold_id(scope.hash(), lumen_core::identity::hash_id("local")),
        scope.hash(),
        || "row/local".to_string(),
        || 0,
    );
    local.set(h.runtime(), 42);
    h.pump();
    assert_eq!(label(&h, "row"), "v=0 local=42");

    // Change the deps: the closure re-runs, the local state stays.
    let v: Signal<i64> = h.runtime().signal("v", || 0);
    v.set(h.runtime(), 3);
    h.pump();
    assert_eq!(
        label(&h, "row"),
        "v=3 local=42",
        "a deps change must re-run the scope without resetting its own state"
    );
}
