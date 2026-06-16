//! M5-exit release gate (desktop leg): an agent, through lumen-agent only,
//! drives the localized, routed, form-driven CRUD app — adds/deletes/undoes a
//! contact, navigates the back stack, switches to an RTL locale — and exports a
//! passing test from its own session. The web + Android legs are added by
//! `scripts/agent_gauntlet_web.sh`.

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
fn m5_gauntlet_desktop() {
    let mut app = agent_gauntlet_web::main_app().run_headless(Size::new(420.0, 480.0));
    app.pump();
    assert!(tree(&mut app).contains("Contacts"));
    assert!(tree(&mut app).contains("(no contacts)"));

    // Route to the add screen; an empty save is blocked (form validation).
    click(&mut app, "#add");
    assert!(tree(&mut app).contains("required"), "required error shown");
    click(&mut app, "#commit");
    assert!(
        tree(&mut app).contains("Name"),
        "still on add screen (invalid)"
    );

    // Fill a valid name and commit → back on the list with the contact.
    rpc(
        &mut app,
        "input.type",
        json!({ "selector": "#newname-input", "text": "Ada" }),
    );
    click(&mut app, "#commit");
    assert!(
        tree(&mut app).contains("1. Ada"),
        "contact added + routed back"
    );

    // Delete it, then undo → it comes back.
    click(&mut app, "#delete");
    assert!(tree(&mut app).contains("(no contacts)"));
    click(&mut app, "#undo");
    assert!(
        tree(&mut app).contains("1. Ada"),
        "undo restored the contact"
    );

    // Localize: switch to Arabic (title) and mirror the layout RTL.
    click(&mut app, "#locale");
    assert!(
        tree(&mut app).contains("جهات الاتصال"),
        "title localized to Arabic"
    );
    let r = rpc(&mut app, "input.setLocale", json!({ "locale": "ar" }));
    assert_eq!(r["result"]["rtl"], json!(true), "layout mirrored RTL");
    assert!(app.is_rtl());

    // Export a regression test from this very session.
    let mut session = Session::new();
    session.dispatch(
        &mut app,
        &json!({ "id": 1, "method": "input.click", "params": { "selector": "#add" } }),
    );
    let export = session.dispatch(
        &mut app,
        &json!({ "id": 2, "method": "session.exportTest",
                 "params": { "fnName": "m5_replay", "appExpr": "agent_gauntlet_web::main_app()" } }),
    );
    let src = export["result"]["source"].as_str().unwrap();
    assert!(src.contains("fn m5_replay()") && src.contains(r##"app.locator("#add").click()"##));
}
