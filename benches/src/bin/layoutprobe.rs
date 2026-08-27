//! Where does a relayout of an unchanged tree actually go?
//!
//! An animated frame recomputes the whole layout every tick. Taffy caches, so
//! a second `compute` over an unchanged tree should be nearly free — unless
//! something unconditional runs beside it. `update_abs` is that something: it
//! walks every node, calls `taffy.children()` (which allocates a `Vec` per
//! node) and inserts into a `HashMap` keyed by node.

use kurbo::Size;
use lumen_layout::{Dim, LayoutStyle, LayoutTree};
use std::time::Instant;

const ROWS: usize = 3000;

fn main() {
    let leaf = LayoutStyle {
        width: Dim::px(200.0),
        height: Dim::px(20.0),
        ..LayoutStyle::default()
    };
    let mut t = LayoutTree::new();
    let kids: Vec<_> = (0..ROWS).map(|_| t.leaf(leaf.clone())).collect();
    let root = t.container(LayoutStyle::default(), &kids);

    // First pass: everything is dirty.
    let s = Instant::now();
    t.compute(root, Size::new(200.0, 200_000.0));
    let first = s.elapsed().as_secs_f64() * 1e6;

    // Subsequent passes: nothing changed at all. Taffy's cache should make the
    // layout itself nearly free, so whatever remains is the unconditional work.
    let mut times = Vec::new();
    for _ in 0..40 {
        let s = Instant::now();
        t.compute(root, Size::new(200.0, 200_000.0));
        times.push(s.elapsed().as_secs_f64() * 1e6);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());

    println!("\nRelayout of an UNCHANGED {ROWS}-row tree");
    println!("──────────────────────────────────────────────");
    println!("  first (all dirty)     : {first:>9.1} us");
    println!("  repeat, min of 40     : {:>9.1} us", times[0]);
    println!("  repeat, median        : {:>9.1} us", times[20]);
    println!("  nodes touched         : {:>9}", t.touched());
    println!(
        "  per node on a repeat  : {:>9.2} us",
        times[0] / t.touched() as f64
    );
    println!();
}
