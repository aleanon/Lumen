//! C.1a (docs/plan-remediation-2026-07.md): agent actions auto-wait. A node
//! that appears only after an async resource resolves is clickable without
//! explicit polling; `ui.waitFor` blocks on existence/text; misses time out
//! with a readable error instead of hanging.

use kurbo::Size;
use lumen_core::tasks::ThreadPoolSpawner;
use lumen_widgets::{widgets, App, BuildCx, Element};
use serde_json::json;

/// The `#late` button exists only once a pool-thread resource (80 ms) lands.
fn build(cx: &mut BuildCx) -> Element {
    let r = cx.resource_blocking::<String, lumen_widgets::TaskError, _>("slow", (), |()| {
        std::thread::sleep(std::time::Duration::from_millis(80));
        Ok("ready".to_string())
    });
    match r.value {
        Some(v) => widgets::column(vec![widgets::button(v, |_| {}).id("late")]),
        None => widgets::text("loading…"),
    }
}

fn call(
    h: &mut lumen_widgets::Headless<lumen_widgets::DefaultRenderer, ThreadPoolSpawner>,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    let req = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    lumen_agent::dispatch(h, &req)
}

#[test]
fn click_auto_waits_for_async_appearance() {
    let mut h = App::new(build)
        .with_executor(ThreadPoolSpawner::new(2))
        .run_headless(Size::new(300.0, 200.0));
    h.pump(); // kicks off the fetch; the button is absent right now
    let resp = call(&mut h, "input.click", json!({ "selector": "#late" }));
    assert_eq!(
        resp["result"]["ok"],
        json!(true),
        "waited then clicked: {resp}"
    );
}

#[test]
fn wait_for_blocks_on_text() {
    let mut h = App::new(build)
        .with_executor(ThreadPoolSpawner::new(2))
        .run_headless(Size::new(300.0, 200.0));
    h.pump();
    let resp = call(
        &mut h,
        "ui.waitFor",
        json!({ "selector": "#late", "text": "ready" }),
    );
    assert_eq!(resp["result"]["ok"], json!(true), "{resp}");
    assert_eq!(resp["result"]["label"], json!("ready"), "{resp}");
}

#[test]
fn misses_time_out_with_readable_errors() {
    let mut h = App::new(build)
        .with_executor(ThreadPoolSpawner::new(2))
        .run_headless(Size::new(300.0, 200.0));
    h.pump();
    // Unknown selector: bounded, readable timeout instead of a hang.
    let resp = call(
        &mut h,
        "input.click",
        json!({ "selector": "#never", "timeout_ms": 60 }),
    );
    let msg = resp["error"]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("Timeout") && msg.contains("60"),
        "readable timeout: {resp}"
    );
    let resp = call(
        &mut h,
        "ui.waitFor",
        json!({ "selector": "#late", "state": "disabled", "timeout_ms": 120 }),
    );
    let msg = resp["error"]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("Timeout") && msg.contains("disabled"),
        "waitFor timeout names the unmet condition: {resp}"
    );
}
