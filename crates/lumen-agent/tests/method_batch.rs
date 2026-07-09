//! C.4a (docs/plan-remediation-2026-07.md): the mechanical method batch —
//! `state.get`, subtree `ui.getTree {selector}`, screenshot `max_width`,
//! `input.hover`, click `button`/`count`, scroll `dx`, `input.type {clear}`.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};
use serde_json::json;

fn build(cx: &mut BuildCx) -> Element {
    let n = cx.signal("n", || 7i64);
    let _ = n.get(cx.runtime());
    widgets::column(vec![
        widgets::text("hello").id("t"),
        // The full editor (select-all/clipboard bindings) — `clear` relies
        // on its Ctrl+A; the pre-IME `text_field_basic` appends only.
        lumen_widgets::TextInput::new(cx, "f", "ab").id("f").into(),
        widgets::button("b", |_| {}).id("b"),
    ])
}

fn call(
    h: &mut lumen_widgets::Headless,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    lumen_agent::dispatch(
        h,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }),
    )
}

#[test]
fn state_get_reads_the_store() {
    let mut h = App::new(build).run_headless(Size::new(300.0, 200.0));
    h.pump();
    let whole = call(&mut h, "state.get", json!({}));
    assert!(
        whole["result"]["state"].to_string().contains('7'),
        "{whole}"
    );
    let one = call(&mut h, "state.get", json!({ "key": "n" }));
    assert_eq!(one["result"]["value"], json!(7), "{one}");
}

#[test]
fn get_tree_selector_returns_the_subtree() {
    let mut h = App::new(build).run_headless(Size::new(300.0, 200.0));
    h.pump();
    let sub = call(&mut h, "ui.getTree", json!({ "selector": "#b" }));
    assert_eq!(sub["result"]["root"]["role"], json!("button"), "{sub}");
    assert!(
        sub["result"]["root"].get("children").is_some(),
        "subtree shape: {sub}"
    );
}

#[test]
fn screenshot_max_width_downscales_preserving_aspect() {
    let mut h = App::new(build).run_headless(Size::new(300.0, 200.0));
    h.pump();
    let shot = call(&mut h, "ui.screenshot", json!({ "max_width": 150 }));
    assert_eq!(shot["result"]["width"], json!(150), "{shot}");
    assert_eq!(shot["result"]["height"], json!(100), "{shot}");
}

#[test]
fn hover_click_options_and_scroll_axes_route() {
    let mut h = App::new(build).run_headless(Size::new(300.0, 200.0));
    h.pump();
    for (m, p) in [
        ("input.hover", json!({ "selector": "#b" })),
        (
            "input.click",
            json!({ "selector": "#b", "button": "right" }),
        ),
        ("input.click", json!({ "selector": "#b", "count": 2 })),
        ("input.scroll", json!({ "selector": "#b", "dx": -20.0 })),
    ] {
        let r = call(&mut h, m, p.clone());
        assert!(r.get("result").is_some(), "{m} {p} -> {r}");
    }
}

#[test]
fn type_clear_replaces_while_default_appends() {
    let mut h = App::new(build).run_headless(Size::new(300.0, 200.0));
    h.pump();
    call(
        &mut h,
        "input.type",
        json!({ "selector": "#f", "text": "cd" }),
    );
    let v = call(&mut h, "ui.getTree", json!({ "selector": "#f" }));
    assert_eq!(v["result"]["root"]["value"], json!("abcd"), "{v}");

    call(
        &mut h,
        "input.type",
        json!({ "selector": "#f", "text": "xyz", "clear": true }),
    );
    let v = call(&mut h, "ui.getTree", json!({ "selector": "#f" }));
    assert_eq!(v["result"]["root"]["value"], json!("xyz"), "{v}");
}
