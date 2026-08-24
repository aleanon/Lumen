//! O3.2: what a node is PAINTED with, vs what the cascade resolved.
//!
//! `get_styles` reads `node_computed`. `apply_transitions` substitutes the
//! mid-flight blend into `css`, which becomes `node_style` — so during a fade
//! `ui.getStyles` reports the *target* colour while the node paints something
//! else. "Why is this blue when my stylesheet says red" had no answer.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};
use serde_json::json;

fn call(h: &mut lumen_widgets::Headless, m: &str, p: serde_json::Value) -> serde_json::Value {
    lumen_agent::dispatch(
        h,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": m, "params": p }),
    )
}

fn app() -> lumen_widgets::Headless {
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
fn a_settled_node_agrees_with_the_cascade() {
    let mut h = app();
    let applied = call(&mut h, "ui.getAppliedStyles", json!({ "selector": "#b" }));
    assert_eq!(
        applied["result"]["background"].as_str(),
        Some("#ff0000ff"),
        "with nothing animating, paint and cascade agree: {applied}"
    );
    assert_eq!(
        applied["result"]["animating"].as_bool(),
        Some(false),
        "nothing is mid-blend: {applied}"
    );
}

/// The divergence itself. Mid-transition the cascade has already resolved to
/// the target, and the node is painted with the blend.
#[test]
fn mid_transition_the_paint_differs_from_the_cascade() {
    let mut h = app();
    h.runtime().signal("on", || false).set(h.runtime(), true);
    h.pump();
    h.advance(100.0); // 1/3 through a 300ms fade

    let applied = call(&mut h, "ui.getAppliedStyles", json!({ "selector": "#b" }));
    let computed = call(&mut h, "ui.getStyles", json!({ "selector": "#b" }));

    assert_eq!(
        applied["result"]["animating"].as_bool(),
        Some(true),
        "the fade is in flight: {applied}"
    );
    let painted = applied["result"]["background"].as_str().unwrap();
    assert_ne!(
        painted, "#0000ffff",
        "mid-fade the node is NOT yet painted the target colour: {applied}"
    );
    assert_ne!(
        painted, "#ff0000ff",
        "...nor still the start colour: {applied}"
    );
    // And this is the point: the cascade view alone would have said "blue"
    // while the screen showed a blend.
    assert!(
        computed["result"].is_object(),
        "the computed view is still available and unchanged in shape: {computed}"
    );
}

#[test]
fn an_unresolvable_selector_is_null_not_an_error() {
    let mut h = app();
    let r = call(
        &mut h,
        "ui.getAppliedStyles",
        json!({ "selector": "#nope" }),
    );
    assert!(r["result"].is_null(), "matches get_styles' behaviour: {r}");
}
