//! Throwaway timing harness: `cargo test -p chrono-stopwatch --test perf -- --ignored --nocapture`
//! (add `--release` for a realistic number).
use lumen_agent::dispatch;
use serde_json::json;
use std::time::Instant;

#[test]
#[ignore]
fn time_running_pumps() {
    let mut a = chrono_stopwatch::run_headless();
    a.pump();
    dispatch(
        &mut a,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "input.click", "params": { "selector": "#toggle" } }),
    );
    // Warm up.
    for _ in 0..10 {
        a.advance(16.0);
    }
    let n = 120;
    let t = Instant::now();
    for _ in 0..n {
        a.advance(16.0); // advance clock + pump (one frame)
    }
    let per = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
    println!(
        "\nchrono pump (running): {per:.2} ms/frame  ->  ~{:.0} fps",
        1000.0 / per
    );
}
