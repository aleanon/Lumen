//! EXPLAIN1: `ui.explain` — one introspection primitive answering the questions
//! that previously had no answer at all.
//!
//! Every case here is a *silent* failure: the UI reports success, nothing
//! happens, and no diagnostic fires. Those are the ones that cost an agent a
//! debugging loop, so each assertion checks that the specific `code` is present
//! — an agent branches on the code, not on prose.

use lumen_agent::dispatch;
use lumen_core::geometry::Size;
use lumen_widgets::{widgets, App, BuildCx};
use serde_json::json;

fn call(
    app: &mut lumen_widgets::Headless,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    let req = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    dispatch(app, &req)["result"].clone()
}

fn codes(v: &serde_json::Value) -> Vec<String> {
    v["reasons"]
        .as_array()
        .map(|rs| {
            rs.iter()
                .filter_map(|r| r["code"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn explains_a_disabled_button_that_swallows_clicks() {
    let mut app = App::new(|_cx: &mut BuildCx| {
        widgets::column(vec![lumen_widgets::Button::new("Go")
            .on_press(|_rt| {})
            .disabled(true)
            .id("go")
            .into()])
    })
    .run_headless(Size::new(200.0, 120.0));
    app.pump();

    let out = call(&mut app, "ui.explain", json!({ "selector": "#go" }));
    assert!(
        codes(&out).contains(&"disabled".to_string()),
        "a disabled button must be reported as such, got {out}"
    );
}

/// A healthy button yields NO reasons. That is a real answer, not a failure to
/// diagnose: it tells the agent to look at its own handler rather than the UI.
#[test]
fn a_clickable_button_reports_nothing_wrong() {
    let mut app = App::new(|_cx: &mut BuildCx| {
        widgets::column(vec![widgets::button("Go", |_rt| {}).id("go")])
    })
    .run_headless(Size::new(200.0, 120.0));
    app.pump();

    let out = call(&mut app, "ui.explain", json!({ "selector": "#go" }));
    assert!(
        codes(&out).is_empty(),
        "a healthy button should yield no reasons, got {out}"
    );
}

/// Clicking a plain container does nothing *by design* — but that is
/// indistinguishable from a broken handler without being told.
#[test]
fn explains_that_a_container_has_no_click_action() {
    let mut app = App::new(|_cx: &mut BuildCx| {
        widgets::column(vec![widgets::text("just text").id("lbl")]).id("root")
    })
    .run_headless(Size::new(200.0, 120.0));
    app.pump();

    let out = call(&mut app, "ui.explain", json!({ "selector": "#lbl" }));
    assert!(
        codes(&out).contains(&"no_click_action".to_string()),
        "a non-interactive node must say so, got {out}"
    );
}

/// The `.lss` blind spot: a property the parser accepts and the runtime never
/// applies. Nothing in the stylesheet, the diagnostics or the rendered frame
/// distinguishes it from a selector that simply did not match.
#[test]
fn explains_that_a_property_parses_but_is_never_applied() {
    let mut app = App::new(|_cx: &mut BuildCx| {
        widgets::column(vec![widgets::text("x").id("lbl")]).id("root")
    })
    .run_headless(Size::new(200.0, 120.0));
    app.set_stylesheet("#lbl { z-index: 3; }");
    app.pump();

    let out = call(
        &mut app,
        "ui.explain",
        json!({ "selector": "#lbl", "kind": "style", "property": "z-index" }),
    );
    assert!(
        codes(&out).contains(&"parse_only".to_string()),
        "`z-index` is parse-only and must be reported as such, got {out}"
    );
}

/// An applied property that simply was not set on this node is a DIFFERENT
/// answer from a parse-only one, and the agent's next move differs: fix the
/// selector, versus stop trying.
#[test]
fn distinguishes_not_matched_from_parse_only() {
    let mut app = App::new(|_cx: &mut BuildCx| {
        widgets::column(vec![widgets::text("x").id("lbl")]).id("root")
    })
    .run_headless(Size::new(200.0, 120.0));
    app.set_stylesheet("#nothing { flex-grow: 1; }");
    app.pump();

    let out = call(
        &mut app,
        "ui.explain",
        json!({ "selector": "#lbl", "kind": "style", "property": "flex-grow" }),
    );
    let c = codes(&out);
    assert!(
        c.contains(&"not_matched".to_string()),
        "an applied-but-unset property must read as not_matched, got {out}"
    );
    assert!(!c.contains(&"parse_only".to_string()));
}

#[test]
fn explains_that_a_text_node_takes_its_height_from_measurement() {
    let mut app = App::new(|_cx: &mut BuildCx| {
        widgets::column(vec![widgets::text("measured").id("lbl")]).id("root")
    })
    .run_headless(Size::new(200.0, 120.0));
    app.pump();

    let out = call(
        &mut app,
        "ui.explain",
        json!({ "selector": "#lbl", "kind": "layout" }),
    );
    assert!(
        codes(&out).contains(&"text_measured".to_string()),
        "a text node's height provenance must be explained, got {out}"
    );
}

/// An unknown kind is an error, not an empty answer — otherwise a typo reads as
/// "nothing is wrong", which is the failure mode this verb exists to remove.
#[test]
fn an_unknown_kind_is_an_error() {
    let mut app = App::new(|_cx: &mut BuildCx| widgets::text("x").id("lbl"))
        .run_headless(Size::new(200.0, 120.0));
    app.pump();
    let req = json!({
        "jsonrpc": "2.0", "id": 1, "method": "ui.explain",
        "params": { "selector": "#lbl", "kind": "nonsense" }
    });
    let resp = dispatch(&mut app, &req);
    assert!(
        resp.get("error").is_some(),
        "an unknown kind must error, got {resp}"
    );
}
