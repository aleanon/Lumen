//! C.2 (docs/plan-remediation-2026-07.md): `app.perf` returns real rolling
//! frame times and `app.logs` exposes the runtime's diagnostic ring —
//! handler-written entries, framework events, `since` paging.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};
use serde_json::json;

fn build(cx: &mut BuildCx) -> Element {
    let n = cx.signal("n", || 0i64);
    let v = n.get(cx.runtime());
    widgets::column(vec![
        widgets::text(format!("v={v}")).id("v"),
        widgets::button("bump", move |rt| {
            rt.log("info", "bump clicked");
            n.update(rt, |x| *x += 1);
        })
        .id("bump"),
    ])
}

fn call(
    h: &mut lumen_widgets::Headless,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    let req = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    lumen_agent::dispatch(h, &req)
}

#[test]
fn perf_reports_real_frame_times() {
    let mut h = App::new(build).run_headless(Size::new(300.0, 200.0));
    h.pump();
    for _ in 0..3 {
        call(&mut h, "input.click", json!({ "selector": "#bump" }));
    }
    let perf = call(&mut h, "app.perf", json!({}));
    let frames = perf["result"]["frames_rendered"].as_u64().unwrap();
    assert!(frames >= 3, "the 3 click frames painted: {perf}");
    let p50 = perf["result"]["frame_ms_p50"].as_f64().unwrap();
    let p95 = perf["result"]["frame_ms_p95"].as_f64().unwrap();
    assert!(p50 > 0.0, "p50 is a real measurement: {perf}");
    assert!(p95 >= p50, "p95 ≥ p50: {perf}");
}

#[test]
fn logs_capture_handlers_and_framework_events_with_paging() {
    let mut h = App::new(build).run_headless(Size::new(300.0, 200.0));
    h.pump();
    call(&mut h, "input.click", json!({ "selector": "#bump" }));
    // A broken stylesheet edit is rejected — and logged.
    let _ = h.set_stylesheet("button { background: }");

    let logs = call(&mut h, "app.logs", json!({}));
    let entries = logs["result"]["entries"].as_array().unwrap().clone();
    assert!(
        entries
            .iter()
            .any(|e| e["message"] == json!("bump clicked") && e["level"] == json!("info")),
        "handler rt.log entry present: {logs}"
    );
    assert!(
        entries.iter().any(|e| e["level"] == json!("warn")
            && e["message"].as_str().unwrap_or("").contains("rejected")),
        "stylesheet rejection logged: {logs}"
    );

    // Paging: `since` = last seq + 1 returns only newer entries.
    let last = entries.last().unwrap()["seq"].as_u64().unwrap();
    let page = call(&mut h, "app.logs", json!({ "since": last + 1 }));
    assert_eq!(
        page["result"]["entries"].as_array().unwrap().len(),
        0,
        "nothing newer yet: {page}"
    );
    h.runtime().log("error", "late entry");
    let page = call(&mut h, "app.logs", json!({ "since": last + 1 }));
    let newer = page["result"]["entries"].as_array().unwrap();
    assert_eq!(newer.len(), 1, "{page}");
    assert_eq!(newer[0]["message"], json!("late entry"));
}
