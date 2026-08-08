//! Why does `build_node` cost more per node in a bigger tree?
//!
//! Localising the residual drift (commit 04d1077) found that **78% of a
//! 3000-row changed frame is `build_node`**, and that it scales with the
//! individual tree's size rather than with process footprint: three 1000-row
//! apps lower 2.17× faster than one 3000-row app at equal total nodes.
//! Allocation was ruled out by `nodecost.rs`'s counting allocator. The leading
//! hypothesis left standing was **cache residency** — a single build's working
//! set outgrowing L2.
//!
//! That hypothesis was recorded as "needs a profiler", and blocked:
//! `perf_event_paranoid` is 4 here and there is no valgrind. But a profiler is
//! the wrong instrument anyway — it would show *where* time goes, not *why the
//! per-node cost changes with N*. A profile of a 3000-row frame and a 500-row
//! frame would look much the same, and the question is precisely what differs.
//!
//! # The experiment
//!
//! If per-node cost is driven by the working set crossing a cache level, then
//! it is a function of **bytes touched**, not of node count — so inflating
//! `Element` must move the knee to *proportionally fewer nodes*.
//!
//! Run this, record `ns/node` against N. Then add padding to `Element`
//! (`crates/lumen-app/src/element.rs`), rebuild, and run it again. The
//! prediction is sharp and falsifiable:
//!
//! * **If cache residency drives it** — the two curves collapse onto one when
//!   plotted against `N × size_of::<Element>()` rather than against `N`. A 2×
//!   bigger Element halves the node count at which cost/node lifts off.
//! * **If it does not** — the padded curve lifts off at the same *N*, and cost
//!   per node is about the tree's shape rather than its bytes. That kills the
//!   cache-residency story, and with it the cache-residency argument for EL.
//!
//! EL (bundling the rarely-set fields out of `Element`) was deprioritized on RSS
//! grounds: 1.22 MB of Tree+Element against ~270 MB process RSS, a 200× ratio
//! that made per-node bytes look irrelevant. Cache residency is a *different*
//! argument for the same work — L2 is 2 MB per P-core on this box, and 3000
//! nodes × 1072 B is 3.2 MB of Elements alone. This measures whether that
//! second argument is real, because the first one is not.

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App};
use std::time::Instant;

/// Flat rows, one reading a root signal — writing it re-runs the whole closure,
/// so every frame is a full lowering pass. Same shape as `nodecost.rs`'s
/// `flat_app`, kept identical so the numbers are comparable across files.
fn flat_app(n: i64) -> App {
    build(n, true)
}

/// `distinct` selects whether every row carries its own string. That is the
/// difference between N distinct shaping-cache entries and one shared entry,
/// with the node count, tree shape and element count held identical — so any
/// gap between the two curves is text-cache pressure and nothing else.
fn build(n: i64, distinct: bool) -> App {
    App::new(move |cx| {
        let bump = cx.signal("n", || 0i64).get(cx.runtime());
        let rows: Vec<_> = (0..n)
            .map(|i| {
                if i == 0 {
                    widgets::text(format!("counter: {bump}"))
                } else if distinct {
                    widgets::text(format!("row {i} (static)"))
                } else {
                    widgets::text("row (static)".to_string())
                }
            })
            .collect();
        widgets::column(rows)
    })
}

fn main() {
    // Min-of-K, not mean: this is a latency question and the distribution has a
    // hard floor with a long right tail (scheduling, frequency). The minimum is
    // the most reproducible statistic available without pinning the CPU. The
    // MOD6 measurement in this campaign produced an *impossible* result when it
    // was taken as a single shot, which is why this defaults high.
    const REPS: usize = 9;
    let elem = std::mem::size_of::<lumen_app::Element>();
    println!("size_of::<Element>() = {elem} B");
    println!(
        "{:>7}  {:>11}  {:>10}  {:>12}",
        "nodes", "frame (µs)", "ns/node", "elem KiB"
    );

    for (label, distinct) in [("distinct strings", true), ("one shared string", false)] {
        println!("--- {label} ---");
        for &n in &[500i64, 1000, 1500, 2000, 3000, 4000, 6000] {
            let mut h = build(n, distinct).run_headless(Size::new(400.0, 400.0));
            // Warm every cache the frame depends on (shape cache, style memo, taffy)
            // so the measurement is steady-state lowering, not first-touch.
            for _ in 0..6 {
                let n: Signal<i64> = h.runtime().signal("n", || 0);
                n.update(h.runtime(), |x| *x += 1);
                h.pump();
            }
            let mut best = f64::MAX;
            for _ in 0..REPS {
                // Write the root-read signal: without this the pump is a no-op and
                // the whole table reads 0.1 us, which is what the first run of this
                // harness produced. A "measurement" of a frame that never rebuilt.
                let n: Signal<i64> = h.runtime().signal("n", || 0);
                n.update(h.runtime(), |x| *x += 1);
                let t = Instant::now();
                h.pump();
                let us = t.elapsed().as_secs_f64() * 1e6;
                if us < best {
                    best = us;
                }
            }
            println!(
                "{:>7}  {:>11.1}  {:>10.1}  {:>12.0}",
                n,
                best,
                best * 1000.0 / n as f64,
                (n as usize * elem) as f64 / 1024.0
            );
        }
    }
    let _ = flat_app(1);
}
