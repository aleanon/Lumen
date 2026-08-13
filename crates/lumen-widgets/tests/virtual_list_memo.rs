//! `VirtualList::memoized` — rows reused when their dep is unchanged.
//!
//! Three things have to hold at once, and only asserting the first two is how a
//! memo ships broken:
//!
//! 1. a changed row updates (correctness),
//! 2. an unchanged row is **not rebuilt** (the point),
//! 3. scrolling still materializes the right window (the memo must not pin rows
//!    that scrolled away).

use lumen_core::geometry::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App, BuildCx, Element, VirtualList};
use std::cell::RefCell;
use std::rc::Rc;

const ROW_H: f64 = 20.0;
const VIEWPORT: f64 = 200.0;
const ITEMS: usize = 1000;

fn label(h: &lumen_widgets::Headless, id: &str) -> String {
    fn find(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<String> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.label.clone());
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    find(&h.semantics_doc().root, id).unwrap_or_default()
}

/// A list whose row content comes from a signal read in the PARENT — the shape
/// that would freeze under a plain `cx.scope` (see `scope_deps.rs`).
/// `built` records which rows actually ran their builder.
fn app(built: Rc<RefCell<Vec<usize>>>) -> App {
    App::new(move |cx: &mut BuildCx| -> Element {
        let bump: i64 = cx.signal("bump", || 0).get(cx.runtime());
        let target: i64 = cx.signal("target", || 0).get(cx.runtime());
        let built = built.clone();
        let vl = VirtualList::memoized(
            cx,
            "vl",
            ITEMS,
            ROW_H,
            VIEWPORT,
            // Only the targeted row's dep moves.
            |i| if i as i64 == target { bump } else { 0 },
            move |i| {
                built.borrow_mut().push(i);
                let v = if i as i64 == target { bump } else { 0 };
                widgets::column(vec![
                    widgets::text(format!("row {i} = {v}")).id(format!("r{i}"))
                ])
            },
        );
        widgets::column(vec![vl.into()]).id("root")
    })
}

#[test]
fn an_unchanged_row_is_not_rebuilt_and_a_changed_one_is() {
    let built = Rc::new(RefCell::new(Vec::new()));
    let mut h = app(built.clone()).run_headless(Size::new(300.0, VIEWPORT));
    h.pump();
    let first_pass = built.borrow().len();
    assert!(
        first_pass >= 10,
        "the first build must materialize the window, got {first_pass}"
    );

    // Change row 3 only.
    built.borrow_mut().clear();
    let target: Signal<i64> = h.runtime().signal("target", || 0);
    target.set(h.runtime(), 3);
    h.pump();
    let bump: Signal<i64> = h.runtime().signal("bump", || 0);
    bump.set(h.runtime(), 1);
    h.pump();

    let ran = built.borrow().clone();
    assert!(
        ran.contains(&3),
        "the changed row must rebuild; rows that ran: {ran:?}"
    );
    assert_eq!(
        label(&h, "r3"),
        "row 3 = 1",
        "and its content must be current — this is the freeze `scope_deps.rs` documents"
    );

    // Everything else in the window held its dep, so it must not have re-run.
    let others: Vec<usize> = ran.iter().copied().filter(|&i| i != 3).collect();
    assert!(
        others.is_empty(),
        "unchanged rows were rebuilt anyway: {others:?} — the memo is not hitting, \
         which is the ADR-021 scope-key trap"
    );
}

/// Scrolling still yields the right window: the memo must not pin rows that
/// scrolled out, nor serve a stale row at a reused index.
#[test]
fn scrolling_materializes_the_new_window() {
    let built = Rc::new(RefCell::new(Vec::new()));
    let mut h = app(built.clone()).run_headless(Size::new(300.0, VIEWPORT));
    h.pump();
    assert_eq!(label(&h, "r0"), "row 0 = 0");

    let y: Signal<f64> = h.runtime().signal("vl", || 0.0);
    y.set(h.runtime(), 400.0); // 20 rows down
    h.pump();

    assert_eq!(
        label(&h, "r0"),
        "",
        "row 0 scrolled out and must no longer be mounted"
    );
    assert_eq!(
        label(&h, "r20"),
        "row 20 = 0",
        "the new window must be materialized"
    );
}

/// The plain constructor is untouched: rows rebuild every frame.
#[test]
fn the_unmemoized_constructor_still_rebuilds_every_row() {
    let built = Rc::new(RefCell::new(Vec::new()));
    let b = built.clone();
    let build = move |cx: &mut BuildCx| -> Element {
        let bump: i64 = cx.signal("bump", || 0).get(cx.runtime());
        let b = b.clone();
        let vl = VirtualList::new(cx, "vl", ITEMS, ROW_H, VIEWPORT, move |i| {
            b.borrow_mut().push(i);
            widgets::column(vec![
                widgets::text(format!("row {i} = {bump}")).id(format!("r{i}"))
            ])
        });
        widgets::column(vec![vl.into()]).id("root")
    };
    let mut h = App::new(build).run_headless(Size::new(300.0, VIEWPORT));
    h.pump();
    let n = built.borrow().len();
    built.borrow_mut().clear();

    let bump: Signal<i64> = h.runtime().signal("bump", || 0);
    bump.set(h.runtime(), 1);
    h.pump();
    assert_eq!(
        built.borrow().len(),
        n,
        "`new` must keep rebuilding every row — memoization is opt-in"
    );
}
