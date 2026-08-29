//! A11Y3: what does the semantics tree cost, with and without the agent-only
//! per-node payload (`ink`, `text_metrics`, `deps`, `type_name`)?
//!
//! The case that matters is **AccessKit attached, no agent** — a shipped app
//! with a screen reader running. It builds and holds the tree; it never reads
//! any of those four fields.
//!
//! Measures the retained footprint rather than build time, because build time
//! is not separable here: `semantics_elided` memoizes behind an `Rc`, and a
//! `pump` with nothing dirty is skipped entirely, so both a naive repeat-call
//! loop and a pump/rebuild loop measure 0 µs. Footprint is the honest quantity
//! and is what the 112-byte-per-node change moves.
use lumen_core::geometry::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};

fn rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))?
                .split_whitespace()
                .nth(1)?
                .parse()
                .ok()
        })
        .unwrap_or(0)
}

fn main() {
    let n: usize = std::env::var("ROWS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000);
    let baseline = rss_kb();
    let mut h = App::new(move |_cx: &mut BuildCx| {
        let kids: Vec<Element> = (0..n).map(|i| widgets::text(format!("row {i}"))).collect();
        widgets::column(kids)
    })
    .run_headless(Size::new(400.0, 600.0));
    h.pump();
    let before_tree = rss_kb();
    // Hold it: this is what an app with an AT attached is carrying.
    let tree = h.semantics_elided();
    let after = rss_kb();
    let count = {
        fn walk(n: &lumen_core::semantics::SemanticsNode) -> usize {
            1 + n.children.iter().map(walk).sum::<usize>()
        }
        walk(&tree)
    };
    println!(
        "semtree\tobs={}\tN={n}\tnodes={count}\tnode_bytes={}\trss_start={baseline}\trss_pre_tree={before_tree}\trss_held={after}\ttree_kb={}",
        cfg!(feature = "obs"),
        std::mem::size_of::<lumen_core::semantics::SemanticsNode>(),
        after.saturating_sub(before_tree)
    );
    std::hint::black_box(&tree);
}
