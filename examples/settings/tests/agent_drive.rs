//! M1-exit acceptance: the settings app is fully drivable through lumen-agent
//! (tab navigation, text input, toggles), reports styles, and hot-reloads.

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
fn tree_str(app: &mut Headless) -> String {
    rpc(app, "ui.getTree", json!({}))["result"].to_string()
}

#[test]
fn settings_app_driven_by_agent() {
    let mut app = settings::main_app().run_headless(Size::new(480.0, 360.0));
    app.pump();

    // Screen 0 (General): notifications + volume.
    assert!(tree_str(&mut app).contains("Notifications"));

    // Navigate to screen 3 (About) via the agent.
    let r = rpc(&mut app, "input.click", json!({ "selector": "tab:nth(3)" }));
    assert_eq!(r["result"]["ok"], json!(true));
    assert!(tree_str(&mut app).contains("Lumen Settings 1.0"));

    // Type into the username field (IME/committed text path).
    rpc(
        &mut app,
        "input.type",
        json!({ "selector": "#username", "text": "alex" }),
    );
    assert!(tree_str(&mut app).contains("alex"), "username not set");

    // Navigate to screen 2 (Appearance) and toggle dark mode.
    rpc(&mut app, "input.click", json!({ "selector": "tab:nth(2)" }));
    assert!(tree_str(&mut app).contains("Dark mode"));
    rpc(&mut app, "input.click", json!({ "selector": "#dark_mode" }));
    assert!(tree_str(&mut app).contains("checked"), "switch not toggled");

    // Styles come from the .lss (the title uses the $accent token).
    let styles = rpc(&mut app, "ui.getStyles", json!({ "selector": "#title" }));
    assert_eq!(styles["result"]["color"]["value"], json!("#1a73e8ff"));

    // Hot-reload the stylesheet live; the agent observes the new style.
    app.set_stylesheet("#title { color: #ff0000ff; }");
    let styles = rpc(&mut app, "ui.getStyles", json!({ "selector": "#title" }));
    assert_eq!(styles["result"]["color"]["value"], json!("#ff0000ff"));
}
