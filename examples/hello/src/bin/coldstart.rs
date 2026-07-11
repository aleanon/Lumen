//! R.6 gate probe: headless cold start (process exec → first painted frame)
//! and, under `LUMEN_MEM_GATE=1`, a leak check (RSS growth over 300
//! signal-write/pump cycles after warm-up). Prints one JSON line.

use std::time::Instant;

fn rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(0)
}

fn main() {
    let t0 = Instant::now();
    let mut h = hello::main_app().run_headless(lumen::geometry::Size::new(800.0, 600.0));
    h.pump();
    let cold_ms = t0.elapsed().as_secs_f64() * 1000.0;

    if std::env::var("LUMEN_MEM_GATE").is_ok() {
        use lumen::state::Signal;
        let n: Signal<i32> = h.runtime().signal("count", || 0i32);
        for _ in 0..20 {
            n.update(h.runtime(), |v| *v += 1);
            h.pump();
        }
        let rss0 = rss_kb();
        for _ in 0..300 {
            n.update(h.runtime(), |v| *v += 1);
            h.pump();
        }
        let growth = rss_kb().saturating_sub(rss0);
        println!("{{\"cold_ms\":{cold_ms:.1},\"rss_growth_kb\":{growth}}}");
    } else {
        println!("{{\"cold_ms\":{cold_ms:.1}}}");
    }
}
