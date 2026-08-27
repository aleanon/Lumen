//! O0.7: `ui.lint {"all": true}` — the cap is the ambient pass's, not the
//! check's.
//!
//! O0.5 capped findings at 50 per code because the per-frame ambient audit
//! cannot afford to format one fact a thousand times a frame. An agent that
//! explicitly asked for a lint and is waiting for the reply is in the opposite
//! position: the cost is bounded by the one request, and a cap could hide the
//! very node it is hunting for. These tests hold the wire contract for both.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};
use serde_json::json;

fn call(h: &mut lumen_widgets::Headless, m: &str, p: serde_json::Value) -> serde_json::Value {
    lumen_agent::dispatch(
        h,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": m, "params": p }),
    )
}

/// A page far taller than its window, so every row past the fold is a W0112
/// offscreen finding — the shape that produced 6372 findings a frame.
fn tall_page() -> lumen_widgets::Headless {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        let rows: Vec<Element> = (0..1000)
            .map(|i| widgets::text(format!("row {i}")).id(format!("r{i}")))
            .collect();
        widgets::column(rows)
    })
    .run_headless(Size::new(200.0, 60.0));
    h.pump();
    h
}

fn findings(v: &serde_json::Value, code: &str) -> usize {
    v["result"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == code)
        .count()
}

#[test]
fn ui_lint_is_capped_by_default_and_says_so() {
    let mut h = tall_page();
    let r = call(&mut h, "ui.lint", json!({}));
    assert_eq!(r["result"]["capped"], json!(true), "default is capped");
    assert!(
        findings(&r, "W0112") < 100,
        "a thousand offscreen rows must not send a thousand findings over the \
         wire; got {}",
        findings(&r, "W0112")
    );
}

#[test]
fn all_true_lifts_the_cap() {
    let mut h = tall_page();
    let capped = call(&mut h, "ui.lint", json!({}));
    let full = call(&mut h, "ui.lint", json!({ "all": true }));

    assert_eq!(full["result"]["capped"], json!(false));
    assert!(
        findings(&full, "W0112") > findings(&capped, "W0112") * 5,
        "uncapped must report far more: {} vs {}",
        findings(&full, "W0112"),
        findings(&capped, "W0112")
    );
}

/// `all: false` is the default spelled out, not a third behaviour — and a
/// non-boolean must not silently uncap.
#[test]
fn a_non_boolean_all_does_not_uncap() {
    let mut h = tall_page();
    for p in [json!({ "all": false }), json!({ "all": "yes" }), json!({})] {
        let r = call(&mut h, "ui.lint", p.clone());
        assert_eq!(r["result"]["capped"], json!(true), "params were {p}");
    }
}
