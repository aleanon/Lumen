//! Does taffy 0.13 re-solve incrementally when only one node is dirty?
//!
//! The 2026-07-03 decision log skipped incremental layout because layout "is
//! one compute_layout that can't be partially re-solved across disjoint
//! subtrees" — decided against taffy 0.7. This asks the same question of 0.13,
//! because the answer decides whether F2's retained node graph is worth
//! anything beyond avoiding node re-creation.
use std::time::Instant;
use taffy::prelude::*;

fn main() {
    const N: usize = 3000;
    let leaf = Style { size: Size { width: length(400.0), height: length(16.0) }, ..Default::default() };
    let col = Style { flex_direction: FlexDirection::Column,
                      size: Size { width: length(400.0), height: auto() }, ..Default::default() };

    let mut t: TaffyTree<()> = TaffyTree::with_capacity(N + 1);
    let kids: Vec<_> = (0..N).map(|_| t.new_leaf(leaf.clone()).unwrap()).collect();
    let root = t.new_with_children(col, &kids).unwrap();
    let avail = Size { width: AvailableSpace::Definite(400.0), height: AvailableSpace::Definite(800.0) };

    // 1. cold solve
    let t0 = Instant::now();
    t.compute_layout(root, avail).unwrap();
    println!("cold compute_layout            {:>9.1} us", t0.elapsed().as_secs_f64() * 1e6);

    // 2. re-solve with NOTHING dirty
    let mut best = f64::MAX;
    for _ in 0..50 {
        let s = Instant::now();
        t.compute_layout(root, avail).unwrap();
        best = best.min(s.elapsed().as_secs_f64() * 1e6);
    }
    println!("re-solve, nothing dirty        {best:>9.1} us");

    // 3. re-solve with ONE leaf dirty
    let mut best1 = f64::MAX;
    for i in 0..50 {
        t.set_style(kids[i % N], Style {
            size: Size { width: length(400.0), height: length(16.0 + (i % 2) as f32) },
            ..Default::default()
        }).unwrap();
        let s = Instant::now();
        t.compute_layout(root, avail).unwrap();
        best1 = best1.min(s.elapsed().as_secs_f64() * 1e6);
    }
    println!("re-solve, ONE leaf dirty       {best1:>9.1} us");

    // 4. re-solve with every leaf dirty
    let mut bestall = f64::MAX;
    for i in 0..20 {
        for k in &kids {
            t.set_style(*k, Style { size: Size { width: length(400.0), height: length(16.0 + (i % 2) as f32) }, ..Default::default() }).unwrap();
        }
        let s = Instant::now();
        t.compute_layout(root, avail).unwrap();
        bestall = bestall.min(s.elapsed().as_secs_f64() * 1e6);
    }
    println!("re-solve, ALL leaves dirty     {bestall:>9.1} us");
}
