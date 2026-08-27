//! AN1 — what one running animation costs a memoized view.
//!
//! `splice_span` refuses a memo hit whose span contains an animating node, and
//! that check walks the span's whole node list. It is gated on an animation
//! being live *anywhere*, so a single spinner made every scope in the view walk
//! its subtree, every frame.
//!
//! One arm per process: this workload is allocation-heavy and criterion timed
//! allocator residue rather than code on it. `min` of many, because the noise
//! here is additive interference.

use lumen_core::geometry::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App};
use std::time::Instant;

fn scopes() -> i64 {
    std::env::var("SCOPES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(400)
}
const WARMUP: usize = 10;
const SAMPLES: usize = 60;

/// One infinite timeline, on one node, in a view of `SCOPES` memoized rows.
const SHEET: &str = "
@keyframes pulse { 0% { background: #000000; } 100% { background: #ffffff; } }
#spinner { animation: pulse 800ms linear 0ms infinite; }
";

fn app() -> App {
    App::new(move |cx| {
        let n = scopes();
        let rows: Vec<_> = (0..n)
            .map(|i| {
                cx.scope(("row", i), move |cx| {
                    let s: Signal<i64> = cx.signal("v", || 0);
                    // `DEPTH` nodes per span: the scan is O(span), the epoch
                    // cache is O(1), so this is where they diverge.
                    let depth: usize = std::env::var("DEPTH")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1);
                    let kids: Vec<_> = (0..depth)
                        .map(|k| widgets::text(format!("row {i}.{k}: {}", s.get(cx.runtime()))))
                        .collect();
                    widgets::column(kids)
                })
            })
            .collect();
        let mut col = widgets::column(rows);
        col.children.push(widgets::text("spin").id("spinner"));
        col
    })
}

fn main() {
    let animated = std::env::args().nth(1).as_deref() != Some("static");
    let mut h = app().run_headless(Size::new(400.0, 600.0));
    if animated {
        h.set_stylesheet(SHEET);
    }
    for _ in 0..4 {
        h.pump();
    }

    // Steady state: nothing changes but the clock, so every scope is a memo
    // hit — which is exactly when the span scan is pure overhead.
    for _ in 0..WARMUP {
        h.advance(16.0);
    }
    let mut us = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let start = Instant::now();
        h.advance(16.0);
        us.push(start.elapsed().as_secs_f64() * 1e6);
    }
    us.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // What is the frame actually DOING? A timing number with no composition
    // beside it is how the last three wrong conclusions happened.
    let st = h.advance(16.0);
    eprintln!(
        "  [{}] nodes={} rebuilt={} copied={} painted={}",
        if animated { "animated" } else { "static" },
        st.node_count,
        st.nodes_rebuilt,
        st.nodes_copied,
        st.painted
    );
    println!(
        "{}\t{:.1}\t{:.1}",
        if animated { "animated" } else { "static" },
        us[0],
        us[SAMPLES / 2]
    );
}
