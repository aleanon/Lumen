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

// --- C.4b -------------------------------------------------------------

#[test]
fn set_value_replaces_a_text_controls_content() {
    let mut h = App::new(build).run_headless(Size::new(300.0, 200.0));
    h.pump();
    let r = call(
        &mut h,
        "app.setValue",
        json!({ "selector": "#f", "value": "replaced" }),
    );
    assert_eq!(r["result"]["ok"], true, "{r}");
    let tree = call(&mut h, "ui.getTree", json!({ "selector": "#f" }));
    assert!(
        tree.to_string().contains("replaced") && !tree.to_string().contains("ab\""),
        "{tree}"
    );
}

#[test]
fn drag_moves_a_slider() {
    let mut h = App::new(|cx: &mut BuildCx| {
        widgets::column(vec![
            lumen_widgets::Slider::new(cx, "v", 0.0, 100.0)
                .id("s")
                .into(),
            widgets::button("target", |_| {}).id("end"),
        ])
    })
    .run_headless(Size::new(400.0, 200.0));
    h.pump();
    let before = call(&mut h, "ui.getTree", json!({ "selector": "#s" }));
    let r = call(
        &mut h,
        "input.drag",
        json!({ "from": "#s", "to": "#end", "steps": 6 }),
    );
    assert_eq!(r["result"]["ok"], true, "{r}");
    let after = call(&mut h, "ui.getTree", json!({ "selector": "#s" }));
    assert_ne!(
        before["result"]["root"]["value"], after["result"]["root"]["value"],
        "drag changed the slider value: {before} -> {after}"
    );
}

#[test]
fn gesture_contract_ok_and_unknown_kind() {
    let mut h = App::new(build).run_headless(Size::new(300.0, 200.0));
    h.pump();
    for kind in ["tap", "double_tap", "long_press", "pan", "pinch"] {
        let r = call(
            &mut h,
            "input.gesture",
            json!({ "selector": "#b", "kind": kind, "dx": 5.0, "scale": 1.5 }),
        );
        assert_eq!(r["result"]["ok"], true, "{kind}: {r}");
    }
    let bad = call(
        &mut h,
        "input.gesture",
        json!({ "selector": "#b", "kind": "wiggle" }),
    );
    assert!(
        bad["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("unknown gesture"),
        "{bad}"
    );
}

#[test]
fn app_command_invokes_registered_commands() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let n = cx.signal("count", || 0i64);
        cx.register_command("increment", move |rt| n.update(rt, |v| *v += 1));
        widgets::column(vec![widgets::text("app").id("t")])
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    let r = call(&mut h, "app.command", json!({ "name": "increment" }));
    assert_eq!(r["result"]["ok"], true, "{r}");
    let v = call(&mut h, "state.get", json!({ "key": "count" }));
    assert_eq!(v["result"]["value"], json!(1), "{v}");

    let bad = call(&mut h, "app.command", json!({ "name": "nope" }));
    assert!(
        bad["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("increment"),
        "unknown-command error lists what exists: {bad}"
    );
}

#[test]
fn reload_apply_swaps_the_sheet_atomically() {
    let mut h = App::new(build)
        .stylesheet("#b { background: #0000ffff; }")
        .run_headless(Size::new(300.0, 200.0));
    h.pump();
    let r = call(
        &mut h,
        "reload.apply",
        json!({ "source": "#b { background: #ff0000ff; }" }),
    );
    assert_eq!(r["result"]["applied"], true, "{r}");
    let styles = call(&mut h, "ui.getStyles", json!({ "selector": "#b" }));
    assert_eq!(
        styles["result"]["background"]["value"], "#ff0000ff",
        "{styles}"
    );
    let broken = call(&mut h, "reload.apply", json!({ "source": "#b { nope" }));
    assert_eq!(broken["result"]["applied"], false, "{broken}");
    let styles = call(&mut h, "ui.getStyles", json!({ "selector": "#b" }));
    assert_eq!(
        styles["result"]["background"]["value"], "#ff0000ff",
        "previous sheet stays live: {styles}"
    );
}

#[test]
fn session_start_stop_bracket_the_recording() {
    let mut h = App::new(build).run_headless(Size::new(300.0, 200.0));
    h.pump();
    let mut s = lumen_agent::Session::new();
    let mut call_s = |h: &mut lumen_widgets::Headless, m: &str, p: serde_json::Value| {
        s.dispatch(
            h,
            &json!({ "jsonrpc": "2.0", "id": 1, "method": m, "params": p }),
        )
    };
    // Recorded by default…
    call_s(&mut h, "input.click", json!({ "selector": "#b" }));
    // …restart discards it; only the bracketed step remains.
    call_s(&mut h, "session.start", json!({}));
    call_s(&mut h, "input.click", json!({ "selector": "#t" }));
    let stop = call_s(&mut h, "session.stop", json!({}));
    assert_eq!(stop["result"]["steps"], json!(1), "{stop}");
    call_s(&mut h, "input.click", json!({ "selector": "#b" })); // not recorded
    let export = call_s(
        &mut h,
        "session.exportTest",
        json!({ "appExpr": "build()" }),
    );
    let code = export["result"]["source"].as_str().unwrap_or_default();
    assert!(code.contains("#t"), "bracketed step exported: {code}");
    assert!(!code.contains("\"#b\""), "outside steps dropped: {code}");
}
