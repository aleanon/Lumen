//! WT-EXP — does `cx.scope` memoization survive without a cloneable `Element`?
//!
//! This is the question that could have killed direct lowering. Memoization is
//! the one place the engine genuinely *retains* an `Element`: a memo-hit stub
//! carries `shared: Option<Rc<Element>>`, and if that is load-bearing then an
//! `Element`-free engine cannot memoize.
//!
//! Reading `splice_span` says otherwise — a memo hit is `detach` +
//! `attach_last_child` on the retained tree, and never looks at an `Element`.
//! These tests hold the sink's version of that to the three properties that
//! matter: a hit reuses nodes, a changed dependency rebuilds only its own
//! scope, and the memoized result is *identical* to a full rebuild.

use lumen_core::semantics::Role;
use lumen_layout::LayoutStyle;
use lumen_widgets::direct::TreeSink;
use lumen_widgets::{Button, Label};

const ROWS: usize = 200;

/// One row: a label whose text is its version, plus a button.
fn row(sink: &mut TreeSink, parent: Option<lumen_core::NodeIndex>, i: usize, ver: u64) -> (lumen_core::NodeIndex, lumen_layout::LayoutNode) {
    let mut open = sink.node(parent, Role::Group).elide(true).resolve();
    let a = open.child(Label::new(format!("row {i} v{ver}")));
    let b = open.child(Button::new("Open"));
    let n = open.index();
    (n, open.end(&LayoutStyle::default(), &[a, b], false))
}

/// Build one frame: `ROWS` memoized scopes, where row `dirty` has a bumped
/// dependency and every other row is unchanged.
fn frame(sink: &mut TreeSink, versions: &[u64]) {
    sink.begin_frame();
    let mut root = sink.node(None, Role::Group).resolve();
    let rn = root.index();
    let mut lns = Vec::with_capacity(ROWS);
    for (i, &ver) in versions.iter().enumerate().take(ROWS) {
        let (_, ln) = root
            .sink()
            .scope(Some(rn), i as u64, ver, move |s, p| row(s, p, i, ver));
        lns.push(ln);
    }
    root.end(&LayoutStyle::default(), &lns, false);
    sink.end_frame();
    sink.assert_balanced();
}

/// The semantic content of the tree, in document order — what the agent sees.
fn snapshot(sink: &TreeSink) -> Vec<(Role, String)> {
    sink.tree
        .subtree_preorder(sink.tree.root())
        .into_iter()
        .filter(|n| sink.tree.is_alive(*n))
        .filter_map(|n| sink.meta.get(&n).map(|m| (m.role, m.label.clone())))
        .collect()
}

#[test]
fn an_unchanged_scope_is_spliced_not_rebuilt() {
    let mut sink = TreeSink::new();
    let versions = vec![1u64; ROWS];
    frame(&mut sink, &versions);
    assert_eq!(sink.stats().rebuilt, ROWS, "first frame builds everything");

    frame(&mut sink, &versions);
    let s = sink.stats();
    assert_eq!(s.spliced, ROWS, "every unchanged scope was reused");
    assert_eq!(s.rebuilt, 0, "no closure ran a second time");
    assert!(
        s.nodes_reused >= ROWS * 3,
        "the reused nodes are whole subtrees, not just their roots: {s:?}"
    );
}

#[test]
fn only_the_changed_scope_rebuilds() {
    let mut sink = TreeSink::new();
    let mut versions = vec![1u64; ROWS];
    frame(&mut sink, &versions);

    versions[73] = 2; // one row's data changed
    frame(&mut sink, &versions);

    let s = sink.stats();
    assert_eq!(s.rebuilt, 1, "exactly the dirty scope re-ran: {s:?}");
    assert_eq!(s.spliced, ROWS - 1, "the rest were spliced");
    // O(changed): the sweep frees the dirty row's old nodes and nothing else.
    assert!(
        s.nodes_freed <= 4,
        "a one-row change freed {} nodes; the sweep is walking live subtrees",
        s.nodes_freed
    );
}

#[test]
fn the_memoized_tree_is_identical_to_a_full_rebuild() {
    // The property that matters most: memoization must be invisible. Build the
    // same content twice — once incrementally with splicing, once from scratch
    // — and compare what the agent would see.
    let mut incremental = TreeSink::new();
    let mut versions = vec![1u64; ROWS];
    frame(&mut incremental, &versions);
    versions[10] = 2;
    versions[150] = 5;
    frame(&mut incremental, &versions);
    frame(&mut incremental, &versions); // a third frame, all splices

    let mut scratch = TreeSink::new();
    frame(&mut scratch, &versions);

    assert_eq!(
        snapshot(&incremental),
        snapshot(&scratch),
        "a spliced tree and a freshly built one must be indistinguishable"
    );
}

#[test]
fn splicing_preserves_sibling_order() {
    // Splicing is `detach` + `attach_last_child`, so order is only preserved
    // because scopes are visited in order. If that were wrong, rows would
    // shuffle — a bug no amount of speed would excuse.
    let mut sink = TreeSink::new();
    let mut versions = vec![1u64; ROWS];
    frame(&mut sink, &versions);
    versions[0] = 9;
    versions[ROWS - 1] = 9;
    frame(&mut sink, &versions);

    let labels: Vec<String> = snapshot(&sink)
        .into_iter()
        .filter(|(r, _)| *r == Role::Text)
        .map(|(_, l)| l)
        .collect();
    let expected: Vec<String> = (0..ROWS)
        .map(|i| format!("row {i} v{}", versions[i]))
        .collect();
    assert_eq!(labels, expected, "rows stayed in document order");
}

#[test]
fn the_tree_does_not_grow_across_frames() {
    // Splice + sweep must be conservative: repeated frames over the same
    // content should leave the node count flat, or the retained tree leaks.
    let mut sink = TreeSink::new();
    let versions = vec![1u64; ROWS];
    frame(&mut sink, &versions);
    let after_first = sink.tree.len();
    for _ in 0..10 {
        frame(&mut sink, &versions);
    }
    assert_eq!(
        sink.tree.len(),
        after_first,
        "ten more frames left the tree the same size"
    );
}
