//! Lumen's arm of the cross-framework benchmark — see `fwbench/SPEC.md`.
//!
//! Same workload as the Qt, GTK, iced and Xilem harnesses: `ROWS` rows, each a
//! label reading `"row <i> · <counter>"`, optionally wrapped in `DEPTH` nested
//! vertical containers. One `build` measurement, then 40 changed frames after
//! 15 warm-up frames, with the counter moving so every label's text is new and
//! no framework can serve it from a shaping cache.

use lumen_core::geometry::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App, Element};
use std::time::Instant;

fn env(k: &str, d: usize) -> usize {
    std::env::var(k)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(d)
}

fn peak_rss_kb() -> i64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(-1)
}

fn main() {
    let rows = env("ROWS", 1000);
    let depth = env("DEPTH", 0);
    let win_h = env("WINH", 600) as f64;

    let t0 = Instant::now();
    let mut h = App::new(move |cx| {
        let n = cx.signal("n", || 0i64).get(cx.runtime());
        let kids: Vec<Element> = (0..rows)
            .map(|i| {
                let mut e: Element = widgets::text(format!("row {i} · {n}"));
                for _ in 0..depth {
                    e = widgets::column(vec![e]);
                }
                e
            })
            .collect();
        widgets::column(kids)
    })
    .run_headless(Size::new(400.0, win_h));
    h.pump();
    let build_ms = t0.elapsed().as_secs_f64() * 1e3;

    let bump = |h: &mut lumen_widgets::Headless| {
        let s: Signal<i64> = h.runtime().signal("n", || 0);
        s.update(h.runtime(), |v| *v += 1);
        h.pump()
    };
    for _ in 0..15 {
        bump(&mut h);
    }
    let mut us = Vec::with_capacity(40);
    let mut nodes = 0;
    for _ in 0..40 {
        let t = Instant::now();
        let st = bump(&mut h);
        us.push(t.elapsed().as_secs_f64() * 1e6);
        nodes = st.node_count;
    }
    us.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "lumen\tN={rows}\tD={depth}\tbuild_ms={build_ms:.1}\tframe_min_us={:.1}\tframe_med_us={:.1}\trss_kb={}\tnodes={nodes}",
        us[0],
        us[us.len() / 2],
        peak_rss_kb()
    );
}
