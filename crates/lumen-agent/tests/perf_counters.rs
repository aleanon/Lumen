//! O1.3: `app.perf` reports why a frame was slow, not just that it was.
//!
//! The counters that decide whether the retained pipeline is working
//! (`nodes_copied`, `style_memo_hits`) were maintained every frame and absent
//! from the protocol; `style_memo_stats()` was a public accessor with zero
//! callers. The blocker was that the per-frame counters are zeroed at the top
//! of EVERY pump, and `ui.waitSettled` ends on idle pumps — so the recommended
//! interact → waitSettled → perf sequence read 0/0 regardless of what happened.
//! These are cumulative, and these tests pin that.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};
use serde_json::json;

fn build(cx: &mut BuildCx) -> Element {
    let n = cx.signal("n", || 0i64);
    let v = n.get(cx.runtime());
    widgets::column(vec![
        widgets::text(format!("v={v}")).id("readout"),
        widgets::button("bump", move |rt| n.update(rt, |x| *x += 1)).id("bump"),
    ])
    .id("root")
}

fn call(h: &mut lumen_widgets::Headless, m: &str, p: serde_json::Value) -> serde_json::Value {
    lumen_agent::dispatch(
        h,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": m, "params": p }),
    )
}

#[test]
fn counters_survive_the_idle_pumps_that_wait_settled_ends_on() {
    let mut h = App::new(build).run_headless(Size::new(300.0, 200.0));
    h.pump();

    let before = call(&mut h, "app.perf", json!({}))["result"].clone();
    call(&mut h, "input.click", json!({ "selector": "#bump" }));
    // The exact sequence the tooling recommends: settle, THEN measure. This is
    // what zeroed the per-frame counters — waitSettled pumps until quiescent.
    call(&mut h, "ui.waitSettled", json!({ "timeout_ms": 1000 }));
    let after = call(&mut h, "app.perf", json!({}))["result"].clone();

    let delta = |k: &str| {
        after[k]
            .as_u64()
            .unwrap_or_else(|| panic!("{k} missing: {after}"))
            - before[k].as_u64().unwrap()
    };
    assert!(
        delta("nodes_rebuilt_total") > 0,
        "the click rebuilt nodes; a cumulative counter must show it AFTER \
         waitSettled: before={before} after={after}"
    );
}

#[test]
fn perf_reports_the_session_facts_that_explain_a_slow_frame() {
    let mut h = App::new(build).run_headless(Size::new(300.0, 200.0));
    h.pump();
    let p = call(&mut h, "app.perf", json!({}))["result"].clone();

    // Which renderer is live is the single field that answers "why is this
    // slow" -- a silent CPU fallback is an order of magnitude with no other
    // observable signal. These must be readable at any time, not only from a
    // startup log line that a late-attaching agent has already missed.
    assert!(p["renderer"].as_str().is_some(), "renderer named: {p}");
    assert_eq!(
        p["is_gpu"].as_bool(),
        Some(false),
        "the headless default is TinySkia, a CPU backend: {p}"
    );

    // An outlier frame must stay visible after the 120-frame percentile window
    // has forgotten it.
    assert!(p["frame_ms_max"].as_f64().is_some(), "max present: {p}");
    assert!(
        p["frame_budget_ms"].as_f64().unwrap() > 16.0,
        "the budget the over-budget count is measured against: {p}"
    );

    // Cache occupancy: `len` repeatedly at `cap` is the text-thrash tell.
    assert!(p["shape_cache_len"].as_u64().is_some(), "shape cache: {p}");
    assert!(
        p["shape_cache_cap"].as_u64().unwrap() > 0,
        "a real soft cap: {p}"
    );
}

#[test]
fn memo_counters_are_exposed_at_all() {
    // `style_memo_stats()` had zero callers before this.
    let mut h = App::new(build).run_headless(Size::new(300.0, 200.0));
    h.pump();
    let p = call(&mut h, "app.perf", json!({}))["result"].clone();
    for k in ["style_memo_hits", "style_memo_misses", "nodes_copied_total"] {
        assert!(p[k].as_u64().is_some(), "{k} must be reported: {p}");
    }
}

/// O2.5: the backend is a *standing* fact, so it must be queryable at any time.
/// The W0115 advisory that explains its consequence is drained by the first
/// painted frame (`take_diagnostics` clears on read), so an agent attaching
/// later could never recover it from the log ring — the exact
/// hypothesis-required failure this phase exists to remove.
#[test]
fn the_backend_and_its_known_defects_are_queryable_at_any_time() {
    let mut h = App::new(build).run_headless(Size::new(300.0, 200.0));
    // Long after the first painted frame, and after the ring would have rolled.
    for _ in 0..40 {
        h.pump();
    }
    let p = call(&mut h, "app.perf", json!({}))["result"].clone();
    assert_eq!(
        p["backend"].as_str(),
        Some("cpu"),
        "the headless default is TinySkia: {p}"
    );
    assert_eq!(
        p["backend_has_known_defects"].as_bool(),
        Some(false),
        "the CPU reference backend is the golden contract: {p}"
    );
}
