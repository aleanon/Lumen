//! M4-exit release gate: an agent, through `lumen-agent` only, drives the
//! multi-screen styled UI + custom shader, exports a passing test from its own
//! session, and detects + fixes an injected layout bug via structured
//! diagnostics. Desktop is verified here; `scripts/agent_gauntlet.sh` adds the
//! mobile legs.

use lumen_agent::{dispatch, Session};
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
fn click(app: &mut Headless, sel: &str) {
    assert_eq!(
        rpc(app, "input.click", json!({ "selector": sel }))["result"]["ok"],
        json!(true),
        "click {sel}"
    );
}

#[test]
fn agent_gauntlet_desktop() {
    let mut app = agent_gauntlet::main_app().run_headless(Size::new(640.0, 480.0));
    app.pump();

    // --- multi-screen, styled UI -------------------------------------------
    assert!(tree(&mut app).contains("Gauntlet"));
    let styles = rpc(&mut app, "ui.getStyles", json!({ "selector": "#title" }));
    assert!(
        styles["result"].to_string().contains("1a73e8"),
        "title is themed by .lss: {}",
        styles["result"]
    );
    click(&mut app, "tab:nth(2)"); // Shader screen
    assert!(tree(&mut app).contains("Custom shader"));
    assert!(
        tree(&mut app).contains("\"shader\""),
        "shader widget present"
    );
    click(&mut app, "tab:nth(3)"); // Data screen (1000-row windowed grid)
    assert!(tree(&mut app).contains("Data"));

    // --- generate a passing test from the agent's own session --------------
    let mut session = Session::new();
    session.dispatch(
        &mut app,
        &json!({ "id": 1, "method": "input.click", "params": { "selector": "tab:nth(1)" } }),
    );
    let export = session.dispatch(
        &mut app,
        &json!({ "id": 2, "method": "session.exportTest",
                 "params": { "fnName": "gauntlet_replay", "appExpr": "agent_gauntlet::main_app()" } }),
    );
    let src = export["result"]["source"].as_str().unwrap();
    assert!(src.contains("fn gauntlet_replay()"));
    assert!(src.contains(r##"app.locator("tab:nth(1)").click()"##));

    // --- detect + fix the injected layout bug via diagnostics --------------
    click(&mut app, "tab:nth(1)"); // back to Home, where the bug lives
    let diags = rpc(&mut app, "app.diagnostics", json!({}));
    let codes: Vec<String> = diags["result"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap().to_string())
        .collect();
    assert!(
        codes.iter().any(|c| c == "W0103"),
        "layout overflow detected: {codes:?}"
    );

    // The agent applies the fix (removes the too-small fixed box).
    click(&mut app, "#fix");
    let after = rpc(&mut app, "app.diagnostics", json!({}));
    let after_codes: Vec<&str> = after["result"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert!(
        !after_codes.contains(&"W0103"),
        "overflow fixed: {after_codes:?}"
    );
}
