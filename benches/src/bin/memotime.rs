//! WT-EXP — is a memoized frame O(changed) without a cloneable `Element`?
//!
//! One arm per process, for the reason `lowertime.rs` documents: this workload
//! is allocation-heavy and criterion timings tracked heap history rather than
//! code.
//!
//! `full`  — every scope's closure runs (the no-memo baseline).
//! `memo`  — one scope of N is dirty; the rest splice.

use lumen_core::NodeIndex;
use lumen_layout::{LayoutNode, LayoutStyle};
use lumen_widgets::direct::TreeSink;
use lumen_widgets::{Button, Label};
use std::time::Instant;

const ROWS: usize = 500;
const WARMUP: usize = 10;
const SAMPLES: usize = 60;

fn row(sink: &mut TreeSink, parent: Option<NodeIndex>, i: usize, ver: u64) -> (NodeIndex, LayoutNode) {
    let mut open = sink
        .node(parent, lumen_core::semantics::Role::Group)
        .elide(true)
        .resolve();
    let a = open.child(Label::new(format!("row {i} v{ver}")));
    let b = open.child(Button::new("Open"));
    let n = open.index();
    (n, open.end(&LayoutStyle::default(), &[a, b], false))
}

fn frame(sink: &mut TreeSink, versions: &[u64]) {
    sink.begin_frame();
    let mut root = sink.node(None, lumen_core::semantics::Role::Group).resolve();
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
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "memo".into());
    let dirty_all = mode == "full";

    let mut sink = TreeSink::new();
    let mut versions = vec![1u64; ROWS];
    frame(&mut sink, &versions);

    let mut bump = 0u64;
    let mut step = |sink: &mut TreeSink, versions: &mut Vec<u64>| {
        bump += 1;
        if dirty_all {
            // Every scope's dep changes, so every closure re-runs.
            for v in versions.iter_mut() {
                *v = bump;
            }
        } else {
            // Exactly one row changes.
            versions[(bump as usize) % ROWS] = bump;
        }
        frame(sink, versions);
    };

    for _ in 0..WARMUP {
        step(&mut sink, &mut versions);
    }
    let mut us = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let t = Instant::now();
        step(&mut sink, &mut versions);
        us.push(t.elapsed().as_secs_f64() * 1e6);
    }
    us.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let s = sink.stats();
    println!(
        "{mode}\t{:.1}\t{:.1}\t{:.1}\tspliced={} rebuilt={} reused={} freed={}",
        us[SAMPLES / 2],
        us[SAMPLES / 4],
        us[SAMPLES * 3 / 4],
        s.spliced,
        s.rebuilt,
        s.nodes_reused,
        s.nodes_freed
    );
}
