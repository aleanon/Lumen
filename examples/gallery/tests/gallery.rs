//! T7.2: the gallery self-tests every widget — built-in and third-party —
//! through lumen-agent (the gallery "drives itself").
use lumen_agent::dispatch;
use lumen_core::geometry::Size;
use lumen_widgets::Headless;
use serde_json::{json, Value};

fn rpc(app: &mut Headless, method: &str, params: Value) -> Value {
    dispatch(
        app,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }),
    )
}
fn tree(app: &mut Headless) -> String {
    rpc(app, "ui.getTree", json!({}))["result"].to_string()
}

#[test]
fn gallery_drives_builtin_and_thirdparty_widgets() {
    let mut app = gallery::main_app().run_headless(Size::new(360.0, 320.0));
    app.pump();
    assert!(tree(&mut app).contains("Component gallery"));

    rpc(&mut app, "input.click", json!({ "selector": "#switch" }));
    rpc(&mut app, "input.click", json!({ "selector": "#select" }));
    assert!(tree(&mut app).contains("\"B\""), "select cycled to B");

    // The THIRD-PARTY widget, driven exactly like a built-in: click the 4th star.
    rpc(
        &mut app,
        "input.click",
        json!({ "selector": "#stars-star-3" }),
    );
    assert!(
        tree(&mut app).contains("rating 4 of 5"),
        "third-party rating set to 4"
    );
}
