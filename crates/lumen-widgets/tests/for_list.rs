//! `For` — a materialized list that memoizes in chunks.
//!
//! The contract has two halves and they pull against each other, so both are
//! pinned here: **every item exists** (that is what distinguishes it from
//! `VirtualList`), and **changing one item does not re-render the list** (that
//! is what distinguishes it from a plain `column`).

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App, BuildCx, Element, For};
use std::sync::atomic::{AtomicUsize, Ordering};

const N: usize = 1000;

/// One counter per test: these run in parallel threads, and a shared static
/// would have them resetting each other's counts.
fn app(renders: &'static AtomicUsize) -> lumen_widgets::Headless {
    renders.store(0, Ordering::Relaxed);
    App::new(move |cx: &mut BuildCx| {
        let vals: Vec<i64> = (0..N)
            .map(|i| {
                let v: Signal<i64> = cx.signal(i, || 0);
                v.get(cx.runtime())
            })
            .collect();
        For::new(cx, "rows", &vals, |_cx, i, v| {
            renders.fetch_add(1, Ordering::Relaxed);
            widgets::text(format!("row {i} = {v}")).id(format!("r{i}"))
        })
        .into()
    })
    .run_headless(Size::new(300.0, 400.0))
}

/// The half that separates it from `VirtualList`: nothing is culled.
#[test]
fn every_item_is_materialized() {
    static R: AtomicUsize = AtomicUsize::new(0);
    let mut h = app(&R);
    h.pump();
    assert_eq!(
        R.load(Ordering::Relaxed),
        N,
        "the first build renders every item"
    );
    let doc = h.semantics_json().to_string();
    assert!(doc.contains("row 0 = 0"), "the first item is present");
    assert!(
        doc.contains(&format!("row {} = 0", N - 1)),
        "and so is the last — an item far below the fold, which is the whole \
         reason to use For instead of VirtualList"
    );
    h.assert_view_coherent();
}

/// The half that separates it from a plain `column`: one changed item costs one
/// chunk, not the list.
#[test]
fn changing_one_item_rebuilds_only_its_chunk() {
    static R: AtomicUsize = AtomicUsize::new(0);
    let mut h = app(&R);
    h.pump();
    R.store(0, Ordering::Relaxed);

    let v: Signal<i64> = h.runtime().signal(0usize, || 0);
    v.set(h.runtime(), 42);
    h.pump();

    let n = R.load(Ordering::Relaxed);
    assert!(
        n > 0 && n < N,
        "one item changed: expected one chunk's worth of re-renders, got {n} \
         of {N} — 0 means it did not update at all, {N} means it memoized nothing"
    );
    assert!(
        h.semantics_json().to_string().contains("row 0 = 42"),
        "and the change is on screen"
    );
    h.assert_view_coherent();
}

/// Untouched chunks must not re-render at all — the property the whole widget
/// exists for. Asserted separately from the bound above because "fewer than N"
/// would still pass if it re-rendered 90% of the list.
#[test]
fn untouched_chunks_do_not_re_render() {
    static R: AtomicUsize = AtomicUsize::new(0);
    let mut h = app(&R);
    h.pump();
    R.store(0, Ordering::Relaxed);

    let v: Signal<i64> = h.runtime().signal(0usize, || 0);
    v.set(h.runtime(), 7);
    h.pump();

    let n = R.load(Ordering::Relaxed);
    assert!(
        n <= 256,
        "a chunk is 256 items, so a single change must not re-render more than \
         that; got {n}"
    );
}

/// An idle pump re-renders nothing: the memo holds when no dependency moved.
#[test]
fn an_idle_pump_renders_nothing() {
    static R: AtomicUsize = AtomicUsize::new(0);
    let mut h = app(&R);
    h.pump();
    R.store(0, Ordering::Relaxed);
    h.pump();
    h.pump();
    assert_eq!(
        R.load(Ordering::Relaxed),
        0,
        "nothing changed, so no chunk should have re-run"
    );
}

/// Two lists in one view need distinct ids; with them, they do not collide.
#[test]
fn two_lists_do_not_collide() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let a: Vec<i64> = vec![1, 2, 3];
        let b: Vec<i64> = vec![7, 8, 9];
        let la: Element = For::new(cx, "a", &a, |_c, i, v| {
            widgets::text(format!("a{i}={v}"))
        })
        .into();
        let lb: Element = For::new(cx, "b", &b, |_c, i, v| {
            widgets::text(format!("b{i}={v}"))
        })
        .into();
        widgets::column(vec![la, lb])
    })
    .run_headless(Size::new(300.0, 300.0));
    h.pump();
    let doc = h.semantics_json().to_string();
    assert!(doc.contains("a0=1") && doc.contains("a2=3"), "first list: {doc}");
    assert!(
        doc.contains("b0=7") && doc.contains("b2=9"),
        "second list — a shared id would have made one shadow the other"
    );
    h.assert_view_coherent();
}
