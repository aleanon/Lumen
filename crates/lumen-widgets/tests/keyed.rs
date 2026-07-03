//! F5.1: `widgets::keyed` reactive lists + mark-and-sweep GC. Reordering reuses
//! cached item subtrees; adding/removing touches only the delta; a churning list
//! stays memory-bounded (cache + store), and the view stays coherent throughout.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App, BuildCx};

/// An app rendering `order` (a signal of item ids) as a keyed list; `runs`
/// counts how many times each item's view closure actually executes.
fn list_app(runs: Rc<HashMap<i64, Cell<u32>>>) -> App {
    App::new(move |cx: &mut BuildCx| {
        let order: Signal<Vec<i64>> = cx.signal("order", || vec![1, 2, 3]);
        let ids = order.get(cx.runtime());
        let runs = runs.clone();
        let rows = widgets::keyed(
            cx,
            ids,
            |id| format!("item-{id}"),
            move |cx, id| {
                if let Some(c) = runs.get(id) {
                    c.set(c.get() + 1);
                }
                // Each item owns a scope-local signal (state under its key).
                let n: Signal<i64> = cx.signal("n", || *id * 10);
                widgets::text(format!("{}={}", id, n.get(cx.runtime()))).id("row")
            },
        );
        widgets::column(rows)
    })
}

#[test]
fn reorder_reuses_items_add_remove_touches_delta() {
    // No `assert_view_coherent` here — it calls `rebuild_fresh` (clears caches +
    // reruns every item), so it can't interleave with run-count assertions.
    // Coherence is checked in `keyed_list_stays_coherent`.
    let runs: Rc<HashMap<i64, Cell<u32>>> = Rc::new((1..=5).map(|i| (i, Cell::new(0))).collect());
    let mut h = list_app(runs.clone()).run_headless(Size::new(200.0, 200.0));

    // Initial: items 1,2,3 each ran once.
    assert_eq!((runs[&1].get(), runs[&2].get(), runs[&3].get()), (1, 1, 1));

    let order: Signal<Vec<i64>> = h.runtime().signal("order", Vec::new);

    // Reorder [1,2,3] -> [3,1,2]: no item re-runs (all cached).
    order.set(h.runtime(), vec![3, 1, 2]);
    h.pump();
    assert_eq!(
        (runs[&1].get(), runs[&2].get(), runs[&3].get()),
        (1, 1, 1),
        "reorder reused every cached item"
    );

    // Insert 4: only item 4 runs; 1,2,3 stay cached.
    order.set(h.runtime(), vec![3, 1, 4, 2]);
    h.pump();
    assert_eq!(runs[&4].get(), 1, "the inserted item ran");
    assert_eq!(
        (runs[&1].get(), runs[&2].get(), runs[&3].get()),
        (1, 1, 1),
        "existing items untouched by an insert"
    );

    // Remove 1: no re-runs (removal frees, doesn't run).
    order.set(h.runtime(), vec![3, 4, 2]);
    h.pump();
    assert_eq!(runs[&1].get(), 1, "removed item did not re-run");
}

#[test]
fn keyed_list_stays_coherent() {
    let runs: Rc<HashMap<i64, Cell<u32>>> = Rc::new(HashMap::new());
    let mut h = list_app(runs).run_headless(Size::new(200.0, 200.0));
    let order: Signal<Vec<i64>> = h.runtime().signal("order", Vec::new);
    for next in [vec![3, 1, 2], vec![3, 1, 4, 2], vec![4], vec![5, 4, 6]] {
        order.set(h.runtime(), next);
        h.pump();
        h.assert_view_coherent();
    }
}

#[test]
fn churning_list_stays_bounded() {
    let runs: Rc<HashMap<i64, Cell<u32>>> = Rc::new(HashMap::new());
    let mut h = list_app(runs).run_headless(Size::new(200.0, 200.0));
    let order: Signal<Vec<i64>> = h.runtime().signal("order", Vec::new);

    // Churn: each round shows 3 fresh ids; old items must be swept, not leaked.
    for round in 0..200i64 {
        let base = round * 3;
        order.set(h.runtime(), vec![base, base + 1, base + 2]);
        h.pump();
    }
    h.assert_view_coherent();

    // The store holds only: `order` + the 3 live items' scope-local `n` signals
    // (not ~600). Bounded, not proportional to total churn.
    assert!(
        h.runtime().len() <= 8,
        "store bounded after churn, got {} slots",
        h.runtime().len()
    );
}
