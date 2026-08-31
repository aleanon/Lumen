//! E4: what does the `@direct_bridge` cost against native lowering?
//!
//! A bridged widget builds its whole `Element` subtree — root plus every
//! internal child, in a `Vec` — and hands the tree to `write_tree`. A natively
//! lowered one writes its root, then constructs and writes each internal child
//! one at a time, so the subtree is never materialized and the child `Vec`
//! never allocated.
//!
//! The arms are whole `pump`s over N widgets of one kind, because the question
//! is what the *frame* pays, not what a microbenchmark of one widget shows.
//!
//! MODE=slider|checkbox|label   N=2000   cargo run --release --bin widgetlower

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, CheckBox, Element, Label, Slider, Stack};
use std::time::Instant;

fn env(k: &str, d: usize) -> usize {
    std::env::var(k)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(d)
}

fn main() {
    let n = env("N", 2000);
    let mode = std::env::var("MODE").unwrap_or_else(|_| "slider".into());
    let m = mode.clone();

    // MODE=label_stack: the SAME 2000 leaves, delivered as statement-form
    // children instead of a `Vec<Element>`. Isolates the staging-tree cost —
    // which is where O0.20 measured the win, and is a container/authoring
    // property, not a widget-internals one.
    let t0 = Instant::now();
    if mode == "label_stack" {
        let mut h = App::view(move |cx: &mut BuildCx| {
            let gen: lumen_core::state::Signal<i64> = cx.signal("gen", || 0);
            let _ = gen.get(cx.runtime());
            Stack::column(move |c| {
                for _ in 0..n {
                    c.child(Label::new("opt"));
                }
            })
            .width(lumen_layout::Dim::pct(1.0))
        })
        .run_headless(Size::new(400.0, 600.0));
        let stats = h.pump();
        let build_ms = t0.elapsed().as_secs_f64() * 1e3;
        let mut best = u128::MAX;
        let mut rebuilt = 0u32;
        for i in 0..30 {
            let g: lumen_core::state::Signal<i64> = h.runtime().signal("gen", || 0);
            g.set(h.runtime(), i as i64 + 1);
            let t = Instant::now();
            let st = h.pump();
            best = best.min(t.elapsed().as_micros());
            rebuilt = st.nodes_rebuilt;
        }
        println!(
            "widgetlower\tmode={mode}\tN={n}\tnodes={}\tbuild_ms={build_ms:.1}\t\
             rebuild_min_us={best}\tnodes_rebuilt={rebuilt}\tper_node_ns={:.0}",
            stats.node_count,
            best as f64 * 1000.0 / stats.node_count as f64
        );
        return;
    }
    let mut h = App::new(move |cx: &mut BuildCx| {
        // A structural read that changes every frame, so every pump is a real
        // full rebuild — the path E4 changes.
        let gen: lumen_core::state::Signal<i64> = cx.signal("gen", || 0);
        let _ = gen.get(cx.runtime());
        let names: Vec<String> = (0..n).map(|i| format!("w{i}")).collect();
        let kids: Vec<Element> = (0..n)
            .map(|i| match m.as_str() {
                // Composite: root + track + fill + thumb.
                "slider" => Slider::new(cx, &names[i], 0.0, 100.0).into(),
                // Composite: root + box + label.
                "checkbox" => CheckBox::new(cx, &names[i], "opt").into(),
                // Leaf: one element, the control for "is this just Element cost".
                "label" => Label::new("opt").into(),
                other => panic!("unknown MODE={other}"),
            })
            .collect();
        let mut root: Element = widgets::column(kids);
        root.style.width = lumen_layout::Dim::pct(1.0);
        root
    })
    .run_headless(Size::new(400.0, 600.0));
    let stats = h.pump();
    let build_ms = t0.elapsed().as_secs_f64() * 1e3;

    // Full rebuilds: E4 changes lowering, so force the path that lowers.
    // A resize forces a full rebuild (force_rebuild + cleared view caches),
    // which is exactly the path E4 changes; alternate sizes so no pump is a
    // no-op.
    let mut best = u128::MAX;
    let mut rebuilt = 0u32;
    for i in 0..30 {
        let g: lumen_core::state::Signal<i64> = h.runtime().signal("gen", || 0);
        g.set(h.runtime(), i as i64 + 1);
        let t = Instant::now();
        let st = h.pump();
        best = best.min(t.elapsed().as_micros());
        rebuilt = st.nodes_rebuilt;
    }
    println!(
        "widgetlower\tmode={mode}\tN={n}\tnodes={}\tbuild_ms={build_ms:.1}\t\
         rebuild_min_us={best}\tnodes_rebuilt={rebuilt}\tper_node_ns={:.0}",
        stats.node_count,
        best as f64 * 1000.0 / stats.node_count as f64
    );
}
