//! O1.2: `ui.lastDamage` — what actually repainted last frame.
//!
//! The runtime has computed damage every frame since R2 (it drives the shell's
//! idle-skip and the GPU scissor) and it was reachable from Rust and from the
//! test tracer, but never from the protocol. So "what changed on screen when I
//! clicked that" — the question a human answers by looking — had no answer.

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

fn call(
    h: &mut lumen_widgets::Headless,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    let req = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    lumen_agent::dispatch(h, &req)
}

#[test]
fn a_click_reports_damage_naming_the_node_that_changed() {
    let mut h = App::new(build).run_headless(Size::new(300.0, 200.0));
    h.pump();
    call(&mut h, "input.click", json!({ "selector": "#bump" }));

    let dmg = call(&mut h, "ui.lastDamage", json!({}));
    // Strict: a text-only change must stay a bounded region. Accepting `full`
    // here would let a damage-precision regression pass silently, which is the
    // opposite of what this method exists to expose.
    assert_eq!(
        dmg["result"]["kind"].as_str(),
        Some("region"),
        "a one-node text change must produce bounded damage: {dmg}"
    );

    // Exactly the node whose text changed — not its ancestors, not the button.
    let nodes = dmg["result"]["nodes"].as_array().unwrap();
    let ids: Vec<&str> = nodes.iter().filter_map(|n| n["id"].as_str()).collect();
    assert_eq!(
        ids,
        vec!["readout"],
        "the damage set must name the node that repainted, and only it: {dmg}"
    );
    assert!(
        nodes[0]["node"]
            .as_str()
            .is_some_and(|h| h.starts_with("nx-")),
        "each entry carries an agent handle usable as a selector: {dmg}"
    );

    // Rects alone would force a spatial join the agent has no primitive for,
    // so the rect rides along with the nodes.
    assert!(
        dmg["result"]["rect"]["w"].as_f64().unwrap() > 0.0
            && dmg["result"]["rect"]["h"].as_f64().unwrap() > 0.0,
        "a region must carry a non-empty rect: {dmg}"
    );
}

#[test]
fn an_idle_pump_reports_no_damage() {
    let mut h = App::new(build).run_headless(Size::new(300.0, 200.0));
    h.pump();
    h.pump(); // nothing changed

    let dmg = call(&mut h, "ui.lastDamage", json!({}));
    assert_eq!(
        dmg["result"]["kind"].as_str(),
        Some("none"),
        "an idle pump keeps the retained frame: {dmg}"
    );
    assert!(
        dmg["result"]["rect"].is_null(),
        "no region to report: {dmg}"
    );
    assert!(
        dmg["result"]["nodes"].as_array().unwrap().is_empty(),
        "no nodes repainted: {dmg}"
    );
}
