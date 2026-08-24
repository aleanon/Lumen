//! O3.3: what is moving right now (`ui.animations`), and W0116 for a finite
//! animation that never settles.
//!
//! `is_animating()` and `next_deadline()` existed on `Headless` and appeared
//! NOWHERE in lumen-agent. `ui.waitSettled` uses the underlying condition
//! without ever reporting *what* is animating — so an agent that screenshots
//! mid-transition and diffs against a golden had no way to know it caught a
//! frame in flight.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};
use serde_json::json;

fn call(h: &mut lumen_widgets::Headless, m: &str, p: serde_json::Value) -> serde_json::Value {
    lumen_agent::dispatch(
        h,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": m, "params": p }),
    )
}

fn fading() -> lumen_widgets::Headless {
    let mut h = App::new(|cx: &mut BuildCx| -> Element {
        let on = cx.signal("on", || false);
        let cls = if on.get(cx.runtime()) { "hot" } else { "cold" };
        widgets::column(vec![widgets::text("box").id("b").class(cls)]).id("root")
    })
    .run_headless(Size::new(200.0, 100.0));
    h.set_stylesheet(
        "#b { background: #ff0000ff; transition: background 300ms; } \
         #b.hot { background: #0000ffff; }",
    );
    h.pump();
    h
}

#[test]
fn a_settled_ui_reports_no_animations() {
    let mut h = fading();
    let r = call(&mut h, "ui.animations", json!({}));
    assert!(
        r["result"]["animations"].as_array().unwrap().is_empty(),
        "nothing is moving: {r}"
    );
}

#[test]
fn a_transition_in_flight_is_reported_with_its_progress() {
    let mut h = fading();
    h.runtime().signal("on", || false).set(h.runtime(), true);
    h.pump();
    h.advance(100.0);

    let r = call(&mut h, "ui.animations", json!({}));
    let anims = r["result"]["animations"].as_array().unwrap();
    assert_eq!(anims.len(), 1, "one transition in flight: {r}");
    assert_eq!(anims[0]["node"].as_str(), Some("b"), "names the node: {r}");
    assert_eq!(
        anims[0]["property"].as_str(),
        Some("background"),
        "names the property: {r}"
    );
    let p = anims[0]["progress"].as_f64().unwrap();
    assert!(
        p > 0.0 && p < 1.0,
        "caught mid-flight, which is exactly what a golden diff needs to know: {r}"
    );
    assert_eq!(
        anims[0]["infinite"].as_bool(),
        Some(false),
        "a transition has a declared end: {r}"
    );
}

/// The recalibration the review demanded: an `animation: ... infinite` spinner
/// is working as declared for ANY duration. Warning on elapsed time alone would
/// fire on every slow-but-healthy fetch, and would double up with the
/// resource-pending warning for the same non-bug.
#[test]
fn an_infinite_animation_is_never_overdue() {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![widgets::text("spinner").id("s")]).id("root")
    })
    .run_headless(Size::new(200.0, 100.0));
    h.set_stylesheet(
        "@keyframes spin { 0% { opacity: 0.2; } 100% { opacity: 1; } } \
         #s { animation: spin 800ms infinite; }",
    );
    h.pump();
    h.advance(60_000.0); // a full minute of spinning

    let r = call(&mut h, "ui.animations", json!({}));
    let anims = r["result"]["animations"].as_array().unwrap();
    assert!(!anims.is_empty(), "it is still spinning: {r}");
    assert!(
        anims.iter().all(|a| a["infinite"].as_bool() == Some(true)),
        "an infinite timeline must be flagged as such: {r}"
    );
    assert!(
        anims.iter().all(|a| a["overdue_ms"].as_f64() == Some(0.0)),
        "and must NEVER be overdue: {r}"
    );

    let stuck: Vec<String> = h
        .lint()
        .into_iter()
        .filter(|d| d.code == "W0116")
        .map(|d| d.message)
        .collect();
    assert!(
        stuck.is_empty(),
        "a spinner spinning is not a defect, however long it spins: {stuck:?}"
    );
}
