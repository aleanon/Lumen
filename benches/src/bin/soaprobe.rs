//! Step 3 — is the columnar side table actually better than the record one?
//!
//! `Meta` is 656 bytes of uniform record per node in a `HashMap<NodeIndex,
//! Meta>` — the same shape of problem `Element` was, moved one layer down. This
//! compares it against `MetaStore`'s columns on the two things that matter:
//! bytes per node, and the cost of the semantics walk the agent performs.

use lumen_core::semantics::Role;
use lumen_core::NodeIndex;
use lumen_widgets::direct::{Meta, MetaFlags, MetaStore};
use std::collections::HashMap;
use std::time::Instant;

const N: u32 = 20_000;
const WARMUP: usize = 5;
const SAMPLES: usize = 40;

fn main() {
    // Real arena indices, as the tree hands them out — the store is indexed by
    // them, so synthesising them would not exercise the same slots.
    let mut tree = lumen_core::tree::Tree::new();
    let root = tree.insert_orphan();
    tree.set_root(root);
    let nodes: Vec<NodeIndex> = (0..N).map(|_| tree.insert_child(root)).collect();
    let idx = |i: u32| nodes[i as usize];
    // --- record store, as the prototype has it ---
    let mut records: HashMap<NodeIndex, Meta> = HashMap::default();
    for i in 0..N {
        let mut m = Meta {
            role: Role::Button,
            ..Meta::default()
        };
        m.corner_radius = 4.0;
        m.focusable = true;
        records.insert(idx(i), m);
    }

    // --- columnar store ---
    let mut cols = MetaStore::default();
    for i in 0..N {
        let n = idx(i);
        cols.insert(n, Role::Button);
        cols.set_corner_radius(n, 4.0);
        cols.set_flags(n, MetaFlags::FOCUSABLE, true);
    }

    // A semantics walk: role + flags for every node, which is what the agent's
    // tree projection reads.
    let walk_records = |r: &HashMap<NodeIndex, Meta>| {
        let mut acc = 0usize;
        for i in 0..N {
            if let Some(m) = r.get(&idx(i)) {
                acc += m.role as usize + usize::from(m.focusable);
            }
        }
        acc
    };
    let walk_cols = |c: &MetaStore| {
        let mut acc = 0usize;
        for i in 0..N {
            let n = idx(i);
            if c.contains(n) {
                acc += c.role(n) as usize + usize::from(c.flags(n).contains(MetaFlags::FOCUSABLE));
            }
        }
        acc
    };

    assert_eq!(
        walk_records(&records),
        walk_cols(&cols),
        "the two stores must agree before either is timed"
    );

    let time = |f: &dyn Fn() -> usize| {
        for _ in 0..WARMUP {
            std::hint::black_box(f());
        }
        let mut us = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let t = Instant::now();
            std::hint::black_box(f());
            us.push(t.elapsed().as_secs_f64() * 1e6);
        }
        us.sort_by(|a, b| a.partial_cmp(b).unwrap());
        us[SAMPLES / 2]
    };

    let t_rec = time(&|| walk_records(&records));
    let t_col = time(&|| walk_cols(&cols));

    let rec_bytes = N as usize * std::mem::size_of::<Meta>();
    let col_bytes = cols.column_bytes();

    println!("\nStep 3 — record store vs columns ({N} nodes)");
    println!("──────────────────────────────────────────────────────────────");
    println!("  {:<24}{:>12}{:>14}", "", "HashMap<Meta>", "MetaStore");
    println!(
        "  {:<24}{:>12}{:>14}",
        "bytes/node",
        std::mem::size_of::<Meta>(),
        MetaStore::hot_bytes_per_node()
    );
    println!(
        "  {:<24}{:>11} KiB{:>11} KiB",
        "total",
        rec_bytes / 1024,
        col_bytes / 1024
    );
    println!(
        "  {:<24}{:>11.1} us{:>11.1} us",
        "semantics walk", t_rec, t_col
    );
    println!("──────────────────────────────────────────────────────────────");
    println!(
        "  bytes  : {:.2}x less",
        rec_bytes as f64 / col_bytes.max(1) as f64
    );
    println!("  walk   : {:.2}x faster", t_rec / t_col.max(0.0001));
    println!("  cold records allocated: {} of {N}", cols.cold_count());
    println!();
}
