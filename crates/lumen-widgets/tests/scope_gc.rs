//! Regression: a churning keyed list must not leak scope-local state.
//!
//! `sweep_dead_scopes` sheds the state of a `cx.scope` that vanished from the
//! view. Under string keys that was a prefix match (`evict_prefix("row-3/")`);
//! under hash identity (ADR-021) prefixes don't exist, so it walks recorded
//! slot ownership instead (`Runtime::evict_scope`).
//!
//! Nothing covered this before H0 — which is exactly why the hazard was easy to
//! miss: swapping in hash identity without replacing the eviction mechanism
//! leaks one slot per removed row, silently, with every test still green.

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{App, BuildCx, Element, Headless};

/// A list of `n` rows, each a memoized scope owning one scope-local signal.
fn list_app(n: usize) -> Headless {
    App::new(move |cx: &mut BuildCx| {
        let count: Signal<usize> = cx.signal("count", move || n);
        let rows: Vec<Element> = (0..count.get(cx.runtime()))
            .map(|i| {
                cx.scope(("row", i), move |cx| {
                    // Scope-local state: one signal per row, namespaced under
                    // the row's scope rather than a hand-built key string.
                    let hits: Signal<u32> = cx.signal("hits", || 0);
                    let _ = hits.get(cx.runtime());
                    Element::text(format!("row {i}"))
                })
            })
            .collect();
        col(rows)
    })
    .run_headless(Size::new(200.0, 400.0))
}

fn col(children: Vec<Element>) -> Element {
    let mut el = Element::default();
    el.children = children;
    el
}

#[test]
fn a_churning_list_does_not_leak_scope_local_state() {
    let mut h = list_app(50);
    h.pump();
    let full = h.runtime().len();

    // Drop the list to a single row: 49 rows' scope-local signals must be shed.
    let count: Signal<usize> = h.runtime().signal("count", || 50usize);
    count.set(h.runtime(), 1);
    h.pump();
    let shrunk = h.runtime().len();
    assert!(
        shrunk < full,
        "dropping 49 rows shed nothing: {full} slots before, {shrunk} after — \
         scope-local state leaked"
    );

    // Grow and shrink repeatedly: the store must return to the same size, not
    // creep. A leak shows up here as monotonic growth.
    for round in 0..5 {
        count.set(h.runtime(), 50);
        h.pump();
        count.set(h.runtime(), 1);
        h.pump();
        assert_eq!(
            h.runtime().len(),
            shrunk,
            "store grew after churn round {round}: a vanished row's state survived"
        );
    }
}

#[test]
fn a_surviving_row_keeps_its_state_when_its_neighbours_are_shed() {
    let mut h = list_app(10);
    h.pump();

    // Write into row 0's scope-local signal, addressing it the way the build
    // folded it (root -> ("row", 0) -> "hits").
    let row0 = lumen_core::identity::ScopePath::root().child(("row", 0usize));
    let hits: Signal<u32> = h.runtime().signal_at(
        lumen_core::identity::fold_id(row0.hash(), lumen_core::identity::hash_id("hits")),
        row0.hash(),
        || "row0.hits".to_string(),
        || 0,
    );
    hits.set(h.runtime(), 7);

    // Shed rows 1..10.
    let count: Signal<usize> = h.runtime().signal("count", || 10usize);
    count.set(h.runtime(), 1);
    h.pump();

    assert_eq!(
        hits.get(h.runtime()),
        7,
        "the surviving row lost its state — eviction was too broad"
    );
}
