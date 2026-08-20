//! F2.1's decisive question: when a memo-hit span's taffy nodes are REUSED but
//! their parent chain is freshly minted, does taffy still hit the children's
//! layout cache — or does adoption dirty them?
//!
//! Also checks the ordering hazard: `new_with_children` overwrites the child's
//! parent pointer while the OLD parent still lists it as a child. If taffy's
//! `remove` then clears the child's (already-reassigned) parent pointer, the
//! naive order corrupts the tree.
use std::time::Instant;
use taffy::prelude::*;

fn leaf_style() -> Style {
    Style { size: Size { width: length(400.0), height: length(16.0) }, ..Default::default() }
}
fn col_style() -> Style {
    Style { flex_direction: FlexDirection::Column,
            size: Size { width: length(400.0), height: auto() }, ..Default::default() }
}

fn main() {
    const N: usize = 3000;
    let avail = Size { width: AvailableSpace::Definite(400.0), height: AvailableSpace::Definite(800.0) };

    // ---- baseline: today's shape. clear + re-mint every node, every frame.
    let mut t: TaffyTree<()> = TaffyTree::with_capacity(N + 1);
    let mut best_remint = f64::MAX;
    for _ in 0..30 {
        let s = Instant::now();
        t.clear();
        let kids: Vec<_> = (0..N).map(|_| t.new_leaf(leaf_style()).unwrap()).collect();
        let root = t.new_with_children(col_style(), &kids).unwrap();
        t.compute_layout(root, avail).unwrap();
        best_remint = best_remint.min(s.elapsed().as_secs_f64() * 1e6);
    }
    println!("A. re-mint all {N} + compute      {best_remint:>9.1} us   (today)");

    // ---- F2.1 shape: keep the children, mint only the new parent.
    let mut t2: TaffyTree<()> = TaffyTree::with_capacity(N + 8);
    let kids: Vec<_> = (0..N).map(|_| t2.new_leaf(leaf_style()).unwrap()).collect();
    let mut root = t2.new_with_children(col_style(), &kids).unwrap();
    t2.compute_layout(root, avail).unwrap();
    let want = t2.layout(kids[N - 1]).unwrap().location.y;

    let mut best_reparent = f64::MAX;
    let mut corrupted = false;
    for _ in 0..30 {
        let s = Instant::now();
        // ORDER UNDER TEST: adopt first, then remove the stale parent.
        let new_root = t2.new_with_children(col_style(), &kids).unwrap();
        t2.remove(root).unwrap();
        t2.compute_layout(new_root, avail).unwrap();
        best_reparent = best_reparent.min(s.elapsed().as_secs_f64() * 1e6);
        root = new_root;
        if (t2.layout(kids[N - 1]).unwrap().location.y - want).abs() > 0.01 {
            corrupted = true;
        }
        if t2.child_count(root) != N { corrupted = true; }
    }
    println!("B. reuse {N}, mint 1 + compute    {best_reparent:>9.1} us   (F2.1)");
    println!("   layout still correct?          {}", if corrupted { "NO — CORRUPTED" } else { "yes" });
    println!("   live node count after 30 frames {}", t2.total_node_count());

    // ---- how much of B is compute vs the adopt/remove bookkeeping?
    let mut best_compute = f64::MAX;
    for _ in 0..30 {
        let s = Instant::now();
        t2.compute_layout(root, avail).unwrap();
        best_compute = best_compute.min(s.elapsed().as_secs_f64() * 1e6);
    }
    println!("C. compute only, nothing dirty    {best_compute:>9.1} us");

    // ---- realistic memo frame: one row's style actually changed.
    let mut best_one = f64::MAX;
    for i in 0..30 {
        t2.set_style(kids[7], Style {
            size: Size { width: length(400.0), height: length(16.0 + (i % 2) as f32) },
            ..Default::default() }).unwrap();
        let s = Instant::now();
        let new_root = t2.new_with_children(col_style(), &kids).unwrap();
        t2.remove(root).unwrap();
        t2.compute_layout(new_root, avail).unwrap();
        best_one = best_one.min(s.elapsed().as_secs_f64() * 1e6);
        root = new_root;
    }
    println!("D. F2.1 + one row restyled        {best_one:>9.1} us");
    nested_shape();
}

/// The shape that actually matters: the scope wraps a COLUMN, so the rebuilt
/// parent adopts exactly one child (the span root), not 3000. Called from
/// `main` via the trailing block below.
fn nested_shape() {
    const N: usize = 3000;
    let avail = Size { width: AvailableSpace::Definite(400.0), height: AvailableSpace::Definite(800.0) };
    let mut t: TaffyTree<()> = TaffyTree::with_capacity(N + 8);
    let kids: Vec<_> = (0..N).map(|_| t.new_leaf(leaf_style()).unwrap()).collect();
    let span_root = t.new_with_children(col_style(), &kids).unwrap();
    let mut root = t.new_with_children(col_style(), &[span_root]).unwrap();
    t.compute_layout(root, avail).unwrap();
    let want = t.layout(kids[N - 1]).unwrap().location.y;

    let mut best = f64::MAX;
    let mut bad = false;
    for _ in 0..30 {
        let s = Instant::now();
        let new_root = t.new_with_children(col_style(), &[span_root]).unwrap();
        t.remove(root).unwrap();
        t.compute_layout(new_root, avail).unwrap();
        best = best.min(s.elapsed().as_secs_f64() * 1e6);
        root = new_root;
        if (t.layout(kids[N - 1]).unwrap().location.y - want).abs() > 0.01 { bad = true; }
    }
    println!("E. nested: reuse span, adopt 1    {best:>9.1} us   <- the real F2.1 shape");
    println!("   layout still correct?          {}", if bad { "NO — CORRUPTED" } else { "yes" });
    println!("   live node count                {}", t.total_node_count());

    // And with one row inside the span genuinely restyled.
    let mut best_one = f64::MAX;
    for i in 0..30 {
        t.set_style(kids[7], Style {
            size: Size { width: length(400.0), height: length(16.0 + (i % 2) as f32) },
            ..Default::default() }).unwrap();
        let s = Instant::now();
        let new_root = t.new_with_children(col_style(), &[span_root]).unwrap();
        t.remove(root).unwrap();
        t.compute_layout(new_root, avail).unwrap();
        best_one = best_one.min(s.elapsed().as_secs_f64() * 1e6);
        root = new_root;
    }
    println!("F. nested + one row restyled      {best_one:>9.1} us");
}
