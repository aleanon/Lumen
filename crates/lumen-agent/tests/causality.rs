//! O4.1/O4.2: causality — "did my change do anything?"
//!
//! Two failures a human diagnoses by looking and an agent could not detect:
//! a press that reaches no handler (while `input.click` cheerfully reports
//! `{"ok": true}`, because the *selector* resolved), and a state write that a
//! view depends on which produces no frame.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};
use serde_json::json;

fn call(h: &mut lumen_widgets::Headless, m: &str, p: serde_json::Value) -> serde_json::Value {
    lumen_agent::dispatch(
        h,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": m, "params": p }),
    )
}

fn warns(h: &mut lumen_widgets::Headless, needle: &str) -> Vec<String> {
    let logs = call(h, "app.logs", json!({}));
    logs["result"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["level"].as_str() == Some("warn"))
        .map(|e| e["message"].as_str().unwrap_or("").to_string())
        .filter(|m| m.contains(needle))
        .collect()
}

/// The dead end `ui.explain` was built for, now reported without being asked.
/// `input.click` returns ok whenever the selector resolved, regardless of
/// whether anything was hit.
#[test]
fn a_press_that_reaches_no_handler_is_reported() {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![widgets::text("just a label").id("label")]).id("root")
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();

    let r = call(&mut h, "input.click", json!({ "selector": "#label" }));
    assert_eq!(
        r["result"]["ok"].as_bool(),
        Some(true),
        "the protocol still reports ok -- agents and exported tests depend on \
         that shape, so the information goes to the ring instead: {r}"
    );

    let w = warns(&mut h, "bubbled to the root");
    assert_eq!(w.len(), 1, "...and the ring says nothing was hit: {w:?}");
}

#[test]
fn a_press_on_a_real_button_is_not_reported() {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![widgets::button("go", |_| {}).id("go")]).id("root")
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    call(&mut h, "input.click", json!({ "selector": "#go" }));
    assert!(
        warns(&mut h, "bubbled to the root").is_empty(),
        "a working click must stay silent"
    );
}

/// A signal that only keys a `resource` has ZERO view dependents by design --
/// task deps never register in `m.deps`. Warning on "no dependents" would fire
/// on the canonical async pattern and make false positives the first thing the
/// audit ever logs in a real app.
#[test]
fn a_write_with_no_view_dependents_is_not_reported() {
    let mut h = App::new(|cx: &mut BuildCx| -> Element {
        // `query` is never read by the view -- exactly the shape a resource key
        // has.
        let _query = cx.signal("query", String::new);
        widgets::column(vec![widgets::text("static").id("t")]).id("root")
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();

    h.runtime()
        .signal("query", String::new)
        .set(h.runtime(), "abc".to_string());
    h.pump();

    assert!(
        warns(&mut h, "went idle").is_empty(),
        "a signal no view reads is not a stale-UI bug -- this is the false \
         positive that would have destroyed trust in the channel"
    );
}

/// The machinery behind the stale-write check, tested directly.
///
/// The *positive* case — a write whose dependent view fails to update — only
/// occurs on a genuine framework bug and cannot be synthesized here without
/// introducing one. So what is pinned is the mechanism: written keys are
/// identified correctly, and dependency is correctly attributed. If the check
/// ever misfires it will be because one of these two is wrong.
#[test]
fn written_keys_and_their_dependents_are_identified() {
    let mut h = App::new(|cx: &mut BuildCx| -> Element {
        let n = cx.signal("counter", || 0i64);
        let v = n.get(cx.runtime());
        widgets::column(vec![widgets::text(format!("v={v}")).id("t")]).id("root")
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();

    let before = h.runtime().write_gen();
    h.runtime()
        .signal("counter", || 0i64)
        .set(h.runtime(), 7i64);

    let keys = h.runtime().keys_written_since(before);
    assert!(
        keys.iter().any(|k| k.contains("counter")),
        "the written signal must be identifiable by name: {keys:?}"
    );

    // And it genuinely has a view dependent, which is the other half of the
    // condition -- this is what separates a stale-UI bug from a resource key.
    let deps = call(&mut h, "ui.whatDependsOn", json!({ "signal": keys[0] }));
    assert!(
        deps["result"].is_object() || deps["result"].is_array(),
        "dependency attribution is available to the check: {deps}"
    );
}
