//! F2.1 regression: an outer scope that alternates rebuilt → copied while an
//! inner scope is re-lowered underneath it.
//!
//! taffy's `new_with_children` sets the child's parent pointer but does NOT
//! remove the child from its previous parent's child list. So when a rebuilt
//! container adopts a reused span root and the old container is then freed,
//! taffy nulls the reused node's parent pointer — silently. Freeing that node
//! later leaves its dead key inside a live container, and freeing *that*
//! container panics with "invalid SlotMap key used".
//!
//! Reproduced at the taffy level directly: adopt a reused node into a fresh
//! container, free the old container, then free the reused node while the new
//! container is live — freeing that container afterwards panics.
//!
//! **The runtime is structurally immune, and this test pins the reason.** A
//! span's nodes are only freed when its scope re-runs, and a scope only
//! re-runs when its enclosing scope re-ran — which rebuilds the enclosing
//! container in the *same* frame. So the container holding a freed span root
//! is always doomed too, and the parent-before-child free order drops its
//! child list wholesale before the child is touched. The dangerous middle
//! state (container live, child freed) is unreachable from a view.
//!
//! That invariant is not obvious and is easy to break: any change that frees
//! a span's nodes while its container survives reintroduces the panic. This
//! test drives the alternation that would expose it.

use std::cell::Cell;
use std::rc::Rc;

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{col, widgets, App};

#[test]
fn outer_scope_alternating_rebuilt_and_copied_over_a_relowered_inner_scope() {
    let outer_runs = Rc::new(Cell::new(0u32));
    let o = outer_runs.clone();
    let mut h = App::new(move |cx| {
        // `outer_dep` drives the OUTER scope; `inner_dep` drives the INNER one.
        let outer_dep: Signal<i64> = cx.signal("outer", || 0);
        cx.signal("inner", || 0i64); // declared here, read inside the outer scope
        let od = outer_dep.get(cx.runtime());
        let o = o.clone();
        col![
            widgets::text("spine"),
            cx.scope_with_deps("outer", od, move |cx| {
                o.set(o.get() + 1);
                let idep = cx.runtime().signal("inner", || 0i64);
                let id = idep.get(cx.runtime());
                col![
                    widgets::text(format!("outer {od}")),
                    cx.scope_with_deps("inner", id, move |_cx| {
                        widgets::column(
                            (0..4)
                                .map(|k| widgets::text(format!("inner {id} row {k}")))
                                .collect::<Vec<_>>(),
                        )
                    }),
                ]
            }),
        ]
    })
    .run_headless(Size::new(300.0, 400.0));
    h.pump();

    let outer: Signal<i64> = h.runtime().signal("outer", || 0);
    let inner: Signal<i64> = h.runtime().signal("inner", || 0);

    // Drive the exact alternation the bug needs, several times over: each
    // round rebuilds the outer (so a fresh container adopts the reused inner
    // span), then copies the outer while the inner is re-lowered (so the
    // reused node is freed while a live container still lists it).
    for round in 1..=6i64 {
        outer.set(h.runtime(), round); // outer rebuilt, inner span reused
        h.pump();
        inner.set(h.runtime(), round); // outer copied, inner span re-lowered
        h.pump();
        h.assert_view_coherent();
    }

    assert!(outer_runs.get() >= 6, "the outer scope never re-ran");
}
