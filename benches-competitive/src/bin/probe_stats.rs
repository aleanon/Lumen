//! What does a ONE-ROW change actually cost Lumen, in nodes?
//!
//! `FrameStats` reports `nodes_rebuilt` (lowered fresh) and `nodes_copied`
//! (retained work copied forward from the previous build). In the steady-state
//! benchmark exactly one of N rows changes its text, so an ideal incremental
//! frame rebuilds ~1 and copies ~N-1.
use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App};

fn main() {
    println!("--- plain view (no cx.scope) ---");
    println!("{:>6}  {:>9}  {:>9}  {:>7}  {:>9}", "rows", "rebuilt", "copied", "nodes", "rebuilt%");
    for n in [100usize, 500, 1000, 3000] {
        let mut h = App::new(move |cx| {
            let bump = cx.signal("n", || 0i64).get(cx.runtime());
            let rows: Vec<_> = (0..n)
                .map(|i| {
                    if i == 0 {
                        widgets::text(format!("counter: {bump}"))
                    } else {
                        widgets::text(format!("row {i}"))
                    }
                })
                .collect();
            widgets::column(rows)
        })
        .run_headless(Size::new(400.0, 800.0));
        h.pump();
        let sig: Signal<i64> = h.runtime().signal("n", || 0);
        // steady state: a few frames in, so any first-frame effects are gone
        let mut st = h.pump();
        for _ in 0..5 {
            sig.update(h.runtime(), |v| *v += 1);
            st = h.pump();
        }
        let pct = 100.0 * f64::from(st.nodes_rebuilt) / (st.node_count.max(1) as f64);
        println!(
            "{:>6}  {:>9}  {:>9}  {:>7}  {:>8.1}%",
            n, st.nodes_rebuilt, st.nodes_copied, st.node_count, pct
        );
    }

    println!("\n--- memoized view (cx.scope_with_deps per row) ---");
    println!("{:>6}  {:>9}  {:>9}  {:>7}  {:>9}", "rows", "rebuilt", "copied", "nodes", "rebuilt%");
    for n in [100usize, 500, 1000, 3000] {
        let mut h = App::new(move |cx| {
            let bump = cx.signal("n", || 0i64).get(cx.runtime());
            let rows: Vec<_> = (0..n)
                .map(|i| {
                    // Only row 0's dep moves, so rows 1..n should copy forward.
                    let dep = if i == 0 { bump } else { 0 };
                    cx.scope_with_deps(("row", i), dep, move |_cx| {
                        if i == 0 {
                            widgets::text(format!("counter: {bump}"))
                        } else {
                            widgets::text(format!("row {i}"))
                        }
                    })
                })
                .collect();
            widgets::column(rows)
        })
        .run_headless(Size::new(400.0, 800.0));
        h.pump();
        let sig: Signal<i64> = h.runtime().signal("n", || 0);
        let mut st = h.pump();
        for _ in 0..5 {
            sig.update(h.runtime(), |v| *v += 1);
            st = h.pump();
        }
        let pct = 100.0 * f64::from(st.nodes_rebuilt) / (st.node_count.max(1) as f64);
        println!(
            "{:>6}  {:>9}  {:>9}  {:>7}  {:>8.1}%",
            n, st.nodes_rebuilt, st.nodes_copied, st.node_count, pct
        );
    }
}
