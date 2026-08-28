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

fn row(
    sink: &mut TreeSink,
    parent: Option<NodeIndex>,
    i: usize,
    ver: u64,
) -> (NodeIndex, LayoutNode) {
    let animated = ANIM_ROWS.with(|c| c.get()) > i;
    let mut d = sink
        .node(parent, lumen_core::semantics::Role::Group)
        .id(format!("r{i}"))
        .elide(true);
    if animated {
        d = d.class("anim");
    }
    let mut open = d.resolve();
    let a = open.child_of(Label::new(format!("row {i} v{ver}")));
    let b = open.child_of(Button::new("Open"));
    let n = open.index();
    (n, open.end(&LayoutStyle::default(), &[a, b], false))
}

fn frame(sink: &mut TreeSink, versions: &[u64]) {
    sink.begin_frame();
    let mut root = sink
        .node(None, lumen_core::semantics::Role::Group)
        .resolve();
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

/// A sheet whose revision changes each reload, as a developer's edits do.
fn sheet(rev: u64) -> lumen_widgets::direct::StyleEnv {
    lumen_widgets::direct::StyleEnv::from_source(&format!(
        ".row {{ border-radius: {rev}px; }} button {{ font-weight: 600; }}"
    ))
    .expect("parses")
}

/// A timeline that never ends, on `spinners` of the rows — the Spinner /
/// Skeleton shape. Every span containing one is refused for the life of the
/// app, so this measures what a loading screen costs a memoized frame.
fn animated_sink(spinners: usize) -> TreeSink {
    use lumen_widgets::direct::KeyStop;
    let mut s = TreeSink::new().with_styles(
        lumen_widgets::direct::StyleEnv::from_source(
            ".anim { animation: pulse 800ms linear 0ms infinite; }",
        )
        .expect("parses"),
        lumen_widgets::direct::VisualState::default(),
    );
    s.add_keyframes(
        "pulse",
        vec![
            (
                0.0,
                KeyStop {
                    background: Some(lumen_core::Color::srgb8(0, 0, 0, 255)),
                    ..KeyStop::default()
                },
            ),
            (
                1.0,
                KeyStop {
                    background: Some(lumen_core::Color::srgb8(255, 255, 255, 255)),
                    ..KeyStop::default()
                },
            ),
        ],
    );
    ANIM_ROWS.with(|c| c.set(spinners));
    s
}

thread_local! {
    /// How many leading rows carry the timeline.
    static ANIM_ROWS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "memo".into());
    let dirty_all = mode == "full";
    // `reload` measures the frame after a stylesheet edit: every span is stale,
    // so it is a full rebuild. That is the standing cost of a memo that holds
    // styled nodes rather than the Element model's pre-styling cache.
    let reload = mode == "reload";

    // `anim<N>`: N rows carry an infinite timeline; the rest are memoizable.
    let anim_n = mode
        .strip_prefix("anim")
        .and_then(|n| n.parse::<usize>().ok());
    let mut sink = if let Some(k) = anim_n {
        animated_sink(k)
    } else if reload {
        TreeSink::new().with_styles(sheet(0), lumen_widgets::direct::VisualState::default())
    } else {
        TreeSink::new()
    };
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
        } else if reload {
            // The data is untouched; the developer saved the stylesheet.
            sink.set_stylesheet(sheet(bump));
        } else if anim_n.is_some() {
            // Nothing changes but the clock — the steady state of a loading
            // screen.
            sink.set_clock(bump as f64 * 16.0);
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
