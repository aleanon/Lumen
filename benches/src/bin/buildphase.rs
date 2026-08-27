//! What does a CHANGED frame cost, and how much of it is lowering?
//!
//! `animmemo` measures a memo-hitting frame — `rebuilt=2` — so it cannot say
//! anything about the cost of lowering nodes. This is the opposite shape: every
//! node is rebuilt every frame, which is what direct lowering would actually
//! change.

use lumen_core::geometry::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App};
use std::time::Instant;

fn rows() -> i64 {
    std::env::var("ROWS").ok().and_then(|v| v.parse().ok()).unwrap_or(2000)
}

/// Flat rows all reading one root signal, so a write re-runs the whole closure
/// and every node is lowered fresh. The `nodecost.rs` `flat_app` shape.
fn app() -> App {
    App::new(move |cx| {
        let bump = cx.signal("n", || 0i64).get(cx.runtime());
        let n = rows();
        let rows: Vec<_> = (0..n)
            .map(|i| widgets::text(format!("row {i} · {bump}")))
            .collect();
        widgets::column(rows)
    })
}

fn main() {
    let mut h = app().run_headless(Size::new(400.0, 600.0));
    for _ in 0..5 {
        h.pump();
    }
    let mut bump = |h: &mut lumen_widgets::Headless| {
        let s: Signal<i64> = h.runtime().signal("n", || 0);
        s.update(h.runtime(), |v| *v += 1);
        h.pump()
    };
    for _ in 0..10 {
        bump(&mut h);
    }
    let mut us = Vec::new();
    let mut last = None;
    for _ in 0..40 {
        let t = Instant::now();
        let st = bump(&mut h);
        us.push(t.elapsed().as_secs_f64() * 1e6);
        last = Some(st);
    }
    us.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let st = last.unwrap();
    println!(
        "changed-frame\t{:.1}\t{:.1}\tnodes={} rebuilt={} copied={}",
        us[0],
        us[us.len() / 2],
        st.node_count,
        st.nodes_rebuilt,
        st.nodes_copied
    );
}
