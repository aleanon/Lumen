//! F2 (harness hardening): nested-scope coherence. A memoized *outer* scope must
//! invalidate when an *inner* scope's dep changes (its cached subtree embeds the
//! inner one), while an inner scope still skips independently when only a cousin
//! changed. Plus a deterministic fuzz: random write sequences over many scopes,
//! asserting `assert_view_coherent` throughout — the guardrail the retained-node
//! graph (F2 step 1) will build against.

use std::cell::Cell;
use std::rc::Rc;

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App, BuildCx};

/// Outer scope "o" wrapping inner scope "i" (reads `inner`), beside a sibling
/// scope "s" (reads `sib`). Counters record which closures actually ran.
fn nested(ro: Rc<Cell<u32>>, ri: Rc<Cell<u32>>, rs: Rc<Cell<u32>>) -> App {
    App::new(move |cx: &mut BuildCx| {
        let inner: Signal<i64> = cx.signal("inner", || 0);
        let sib: Signal<i64> = cx.signal("sib", || 0);
        let (ro, ri, rs) = (ro.clone(), ri.clone(), rs.clone());
        widgets::column(vec![
            cx.scope("o", move |cx| {
                ro.set(ro.get() + 1);
                let inner_row = cx.scope("i", |cx| {
                    ri.set(ri.get() + 1);
                    widgets::text(format!("inner={}", inner.get(cx.runtime()))).id("i")
                });
                widgets::column(vec![widgets::text("outer"), inner_row])
            }),
            cx.scope("s", move |cx| {
                rs.set(rs.get() + 1);
                widgets::text(format!("sib={}", sib.get(cx.runtime()))).id("s")
            }),
        ])
    })
}

#[test]
fn inner_change_invalidates_outer_not_sibling() {
    let (ro, ri, rs) = (
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
    );
    let mut h = nested(ro.clone(), ri.clone(), rs.clone()).run_headless(Size::new(200.0, 160.0));
    assert_eq!((ro.get(), ri.get(), rs.get()), (1, 1, 1), "all ran once");

    let inner: Signal<i64> = h.runtime().signal("inner", || 0);
    let sib: Signal<i64> = h.runtime().signal("sib", || 0);

    // Writing the inner signal must re-run the inner scope AND its outer wrapper
    // (whose cached subtree embeds the inner one) — but NOT the sibling.
    inner.set(h.runtime(), 1);
    h.pump();
    assert_eq!(ro.get(), 2, "outer invalidated by inner's dep");
    assert_eq!(ri.get(), 2, "inner re-ran");
    assert_eq!(rs.get(), 1, "sibling untouched");

    // Writing the sibling must re-run only the sibling; outer+inner stay cached.
    sib.set(h.runtime(), 1);
    h.pump();
    assert_eq!(ro.get(), 2, "outer stayed memoized on a cousin change");
    assert_eq!(ri.get(), 2, "inner stayed memoized");
    assert_eq!(rs.get(), 2, "sibling re-ran");
}

#[test]
fn inner_change_stays_coherent() {
    let (ro, ri, rs) = (
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
    );
    let mut h = nested(ro, ri, rs).run_headless(Size::new(200.0, 160.0));
    let inner: Signal<i64> = h.runtime().signal("inner", || 0);
    for i in 1..=3 {
        inner.set(h.runtime(), i);
        h.pump();
        h.assert_view_coherent();
    }
    assert!(h.semantics_json().to_string().contains("inner=3"));
}

/// Deterministic LCG (no `Math.random` / `Date` in this environment).
fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state >> 33
}

#[test]
fn fuzz_random_writes_stay_coherent() {
    const N: i64 = 12;
    let app = App::new(|cx: &mut BuildCx| {
        let rows: Vec<_> = (0..N)
            .map(|i| {
                cx.scope(&format!("row-{i}"), move |cx| {
                    let s: Signal<i64> = cx.signal(&format!("v-{i}"), || 0);
                    widgets::text(format!("row {i} = {}", s.get(cx.runtime()))).id("row")
                })
            })
            .collect();
        widgets::column(rows)
    });
    let mut h = app.run_headless(Size::new(300.0, 400.0));

    let mut seed = 0x1234_5678_9abc_def0u64;
    for _ in 0..60 {
        // Write a random subset (0..3 signals) this round, then pump + verify.
        let k = (lcg(&mut seed) % 3) as usize;
        for _ in 0..k {
            let i = (lcg(&mut seed) as i64) % N;
            let s: Signal<i64> = h.runtime().signal(&format!("v-{i}"), || 0);
            s.update(h.runtime(), |v| *v += 1);
        }
        h.pump();
        h.assert_view_coherent();
    }
}
