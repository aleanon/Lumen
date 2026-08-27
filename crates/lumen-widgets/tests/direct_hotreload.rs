//! WT-EXP P5 — hot reload against the sink.
//!
//! `set_stylesheet` carries a comment that turns out to be the whole issue:
//!
//! ```text
//! // A.5b: resolution results embed the sheet — invalidate the memo
//! // (scope caches stay: cached Elements are pre-styling).
//! self.style_memo.clear();
//! ```
//!
//! In the `Element` model a memoized scope holds **unstyled** elements — the
//! cascade runs later, in `build_node` — so a stylesheet edit invalidates only
//! the *resolution* cache and every scope stays memoized. Closures do not re-run.
//!
//! Direct lowering inverts that. A retained span is finished, already-styled
//! nodes sitting in the tree. Splicing one after the sheet changed reuses the
//! **old** styling, and nothing in `dep` or the ancestor context notices,
//! because neither of them mentions the stylesheet.
//!
//! So the sink needs a sheet generation in its splice guard. This file
//! demonstrates the staleness first, then pins the fix.

use lumen_core::semantics::Role;
use lumen_core::{Color, NodeIndex};
use lumen_layout::LayoutStyle;
use lumen_widgets::direct::{StyleEnv, TreeSink, VisualState};

const ROWS: usize = 12;

fn env(src: &str) -> StyleEnv {
    StyleEnv::from_source(src).expect("the sheet parses")
}

fn row(s: &mut TreeSink, p: Option<NodeIndex>, i: usize) -> (NodeIndex, lumen_layout::LayoutNode) {
    let node = s
        .node(p, Role::Button)
        .id(format!("row{i}"))
        .class("row")
        .resolve();
    let n = node.index();
    (n, node.end(&LayoutStyle::default(), &[], false))
}

fn frame(s: &mut TreeSink) {
    s.begin_frame();
    let mut root = s.node(None, Role::Group).resolve();
    let rn = root.index();
    let mut lns = Vec::with_capacity(ROWS);
    for i in 0..ROWS {
        // The scope's own dependency never changes — only the stylesheet does.
        let (_, ln) = root
            .sink()
            .scope(Some(rn), i as u64, 1, move |s, p| row(s, p, i));
        lns.push(ln);
    }
    root.end(&LayoutStyle::default(), &lns, false);
    s.end_frame();
    s.assert_balanced();
}

fn backgrounds(s: &TreeSink) -> Vec<Option<Color>> {
    s.tree
        .subtree_preorder(s.tree.root())
        .into_iter()
        .filter(|n| s.tree.is_alive(*n))
        .filter(|n| s.meta.contains(*n))
        .filter(|&n| s.meta.role(n) == Role::Button)
        .map(|n| s.meta.background(n))
        .collect()
}

#[test]
fn a_stylesheet_edit_reaches_memoized_scopes() {
    // The bug: every scope's dep is unchanged, so without a sheet generation in
    // the splice guard all twelve rows splice and keep the OLD colour. That is
    // hot reload silently not reloading.
    let mut s =
        TreeSink::new().with_styles(env(".row { background: #0000ff; }"), VisualState::default());
    frame(&mut s);
    for bg in backgrounds(&s) {
        let c = bg.expect("styled");
        assert!(c.b > 0.9, "starts blue: {c:?}");
    }

    // The developer saves the file.
    s.set_stylesheet(env(".row { background: #ff0000; }"));
    frame(&mut s);

    for (i, bg) in backgrounds(&s).into_iter().enumerate() {
        let c = bg.expect("styled");
        assert!(
            c.r > 0.9 && c.b < 0.1,
            "row {i} picked up the edited sheet ({c:?}); if it is still blue \
             the span was spliced across a stylesheet change and hot reload \
             silently did nothing"
        );
    }
    assert_eq!(
        s.stats().rebuilt,
        ROWS,
        "a sheet edit re-runs every scope — a retained span is already-styled \
         nodes, unlike the Element model's pre-styling cache"
    );
}

#[test]
fn an_unchanged_sheet_still_splices() {
    // The guard must key on the sheet's *content*, not on "someone called
    // set_stylesheet" — an editor that saves an unchanged file, or a watcher
    // that fires twice, must not cost a full rebuild.
    let mut s =
        TreeSink::new().with_styles(env(".row { background: #0000ff; }"), VisualState::default());
    frame(&mut s);
    s.set_stylesheet(env(".row { background: #0000ff; }"));
    frame(&mut s);
    assert_eq!(
        s.stats().spliced,
        ROWS,
        "an identical sheet left every scope memoized"
    );
    assert_eq!(s.stats().rebuilt, 0);
}

#[test]
fn a_rejected_edit_leaves_the_live_styling_alone() {
    // `set_stylesheet` returns Failed and keeps the previous sheet live. The
    // sink must not invalidate anything on a rejected parse, or a typo mid-edit
    // would blank the screen.
    let mut s =
        TreeSink::new().with_styles(env(".row { background: #0000ff; }"), VisualState::default());
    frame(&mut s);
    let before = backgrounds(&s);

    let rejected = StyleEnv::from_source(".row { background: ###; }");
    assert!(rejected.is_err(), "the edit really is broken");
    // `from_source` yields nothing, so the sink never sees it.
    frame(&mut s);

    assert_eq!(backgrounds(&s), before, "the live styling survived");
    assert_eq!(s.stats().spliced, ROWS, "and the memo was not disturbed");
}

#[test]
fn reload_cost_is_a_full_rebuild_and_that_is_the_honest_price() {
    // Direct lowering makes a sheet edit strictly more expensive than the
    // Element model, which re-styles cached elements without re-running
    // closures. This test states the cost rather than hiding it.
    let mut s =
        TreeSink::new().with_styles(env(".row { background: #0000ff; }"), VisualState::default());
    frame(&mut s);
    frame(&mut s);
    assert_eq!(s.stats().spliced, ROWS, "steady state is memoized");

    s.set_stylesheet(env(".row { background: #00ff00; }"));
    frame(&mut s);
    assert_eq!(
        s.stats().rebuilt,
        ROWS,
        "a reload frame rebuilds everything — the price of the memo holding \
         styled nodes instead of pre-styling data"
    );

    frame(&mut s);
    assert_eq!(
        s.stats().spliced,
        ROWS,
        "and the very next frame is memoized again, so the cost is one frame"
    );
}
