//! MUT6 measurement: what a full semantics rebuild costs after a patch.
use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{bind, widgets, App, BuildCx, Element};
use std::time::Instant;

fn main() {
    let rows: usize = std::env::var("ROWS").ok().and_then(|v| v.parse().ok()).unwrap_or(50_000);
    let mut h = App::new(move |_cx: &mut BuildCx| {
        let kids: Vec<Element> = (0..rows)
            .map(|i| {
                widgets::text(bind!(rt => {
                    let v: Signal<i64> = rt.signal(i, || 0);
                    format!("row {i} · {}", v.get(rt))
                }))
            })
            .collect();
        let mut root = widgets::column(kids);
        root.style.width = lumen_layout::Dim::pct(1.0);
        root
    })
    .run_headless(Size::new(400.0, 600.0));
    h.pump();
    // Warm, then patch one row and time the semantics rebuild the patch forces.
    let mut best_build = f64::MAX;
    let mut best_cached = f64::MAX;
    for round in 1..=20i64 {
        let v: Signal<i64> = h.runtime().signal(0usize, || 0);
        v.set(h.runtime(), round);
        h.pump(); // patch frame: invalidates sem_root
        let t = Instant::now();
        let _ = h.semantics_elided();
        best_build = best_build.min(t.elapsed().as_secs_f64() * 1e6);
        let t = Instant::now();
        let _ = h.semantics_elided();
        best_cached = best_cached.min(t.elapsed().as_secs_f64() * 1e6);
    }
    println!("semtime N={rows} rebuild_after_patch={best_build:.0}us cached={best_cached:.0}us");
}
