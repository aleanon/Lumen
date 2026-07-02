//! F1: `cx.scope` memoized subtrees. A scope's closure is skipped (its cached
//! subtree reused) while none of the signals it read has changed; changing one
//! scope's signal re-runs only that scope, not its siblings. Coherence with a
//! fresh rebuild is asserted throughout (the F0 guardrail).

use std::cell::Cell;
use std::rc::Rc;

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App, BuildCx};

/// Two independent scopes, each reading its own signal, with a run-counter per
/// scope so a test can see which closure actually executed.
fn two_scopes(runs_a: Rc<Cell<u32>>, runs_b: Rc<Cell<u32>>) -> App {
    App::new(move |cx: &mut BuildCx| {
        let a: Signal<i64> = cx.signal("a", || 0);
        let b: Signal<i64> = cx.signal("b", || 0);
        let ra = runs_a.clone();
        let rb = runs_b.clone();
        widgets::column(vec![
            cx.scope("sa", |cx| {
                ra.set(ra.get() + 1);
                widgets::text(format!("a={}", a.get(cx.runtime()))).id("a")
            }),
            cx.scope("sb", |cx| {
                rb.set(rb.get() + 1);
                widgets::text(format!("b={}", b.get(cx.runtime()))).id("b")
            }),
        ])
    })
}

#[test]
fn changed_scope_reruns_alone() {
    // Note: `assert_view_coherent` calls `rebuild_fresh` (clears caches + reruns
    // every scope), so it can't be interleaved with run-count assertions — it is
    // checked in `changed_scope_stays_coherent` instead.
    let (ra, rb) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let mut h = two_scopes(ra.clone(), rb.clone()).run_headless(Size::new(200.0, 120.0));
    assert_eq!(
        (ra.get(), rb.get()),
        (1, 1),
        "both ran on the initial build"
    );

    let a: Signal<i64> = h.runtime().signal("a", || 0);
    let b: Signal<i64> = h.runtime().signal("b", || 0);

    // Write `a`: only scope A re-runs; B reuses its cache.
    a.set(h.runtime(), 5);
    h.pump();
    assert_eq!(ra.get(), 2, "scope A re-ran on its own signal");
    assert_eq!(rb.get(), 1, "scope B was memoized (not re-run)");

    // Write `b`: now only scope B re-runs; A is memoized.
    b.set(h.runtime(), 7);
    h.pump();
    assert_eq!(ra.get(), 2, "scope A stayed memoized");
    assert_eq!(rb.get(), 2, "scope B re-ran on its own signal");
}

#[test]
fn changed_scope_stays_coherent() {
    let (ra, rb) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let mut h = two_scopes(ra.clone(), rb.clone()).run_headless(Size::new(200.0, 120.0));
    let a: Signal<i64> = h.runtime().signal("a", || 0);
    for i in 1..=3 {
        a.set(h.runtime(), i);
        h.pump();
        h.assert_view_coherent(); // memoized view must equal a fresh rebuild
    }
    assert!(h.semantics_json().to_string().contains("a=3"));
}

#[test]
fn idle_pump_reruns_nothing() {
    let (ra, rb) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let mut h = two_scopes(ra.clone(), rb.clone()).run_headless(Size::new(200.0, 120.0));
    let (a0, b0) = (ra.get(), rb.get());
    h.pump(); // nothing changed
    assert_eq!((ra.get(), rb.get()), (a0, b0), "idle pump re-ran no scope");
}

#[test]
fn scope_namespaces_local_state() {
    // The same inner signal name under two scope ids must be two distinct slots.
    let mut h = App::new(|cx: &mut BuildCx| {
        widgets::column(vec![
            cx.scope("left", |cx| {
                let n: Signal<i64> = cx.signal("n", || 1);
                widgets::text(format!("{}", n.get(cx.runtime()))).id("left")
            }),
            cx.scope("right", |cx| {
                let n: Signal<i64> = cx.signal("n", || 2);
                widgets::text(format!("{}", n.get(cx.runtime()))).id("right")
            }),
        ])
    })
    .run_headless(Size::new(200.0, 120.0));
    let json = h.semantics_json().to_string();
    assert!(
        json.contains('1') && json.contains('2'),
        "two distinct 'n' slots"
    );
    h.assert_view_coherent();
}
