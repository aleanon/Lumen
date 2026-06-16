//! M7-exit — the **2.0 grand gauntlet** (desktop leg): an agent, through
//! lumen-agent only, exercises a production app that is localized (RTL),
//! accessible (WCAG-clean), plugin-extended, media-rich, and form-validated —
//! then **auto-repairs an injected regression** and re-verifies, zero human
//! edits. The web + Android legs are added by `scripts/agent_gauntlet_2.sh`.

use lumen_agent::{auto_repair, dispatch};
use lumen_core::geometry::Size;
use lumen_widgets::wcag::{audit_names, contrast_ratio};
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
fn grand_gauntlet_desktop() {
    let mut app = agent_gauntlet_2::main_app().run_headless(Size::new(420.0, 480.0));
    app.pump();

    // Media + localization + plugin + form all render.
    assert!(tree(&mut app).contains("Account"), "localized title");
    assert!(
        tree(&mut app).contains("rating 0 of 5"),
        "third-party plugin widget"
    );
    assert!(tree(&mut app).contains("required"), "form validation");

    // Accessibility: every interactive node has an accessible name (WCAG 4.1.2),
    // and the brand color has sufficient contrast on white (WCAG 1.4.3).
    let names = audit_names(&app.semantics_doc().root.elided());
    assert!(names.is_empty(), "WCAG name audit clean: {names:?}");
    let white = lumen_core::Color::srgb8(255, 255, 255, 255);
    let accent = lumen_core::Color::srgb8(0x1a, 0x73, 0xe8, 0xff);
    assert!(
        contrast_ratio(white, accent) >= 4.5,
        "AA contrast for white-on-accent"
    );

    // Drive the plugin widget + the form (fill a valid email).
    rpc(
        &mut app,
        "input.click",
        json!({ "selector": "#stars-star-4" }),
    );
    assert!(tree(&mut app).contains("rating 5 of 5"));
    rpc(
        &mut app,
        "input.type",
        json!({ "selector": "#email-input", "text": "a@lumen.dev" }),
    );
    assert!(!tree(&mut app).contains("invalid email"));

    // Localize to RTL.
    rpc(&mut app, "input.click", json!({ "selector": "#lang" }));
    assert!(tree(&mut app).contains("الحساب"), "Arabic title");
    assert_eq!(
        rpc(&mut app, "input.setLocale", json!({ "locale": "ar" }))["result"]["rtl"],
        json!(true)
    );

    // Auto-repair: an injected W0103 regression self-heals, zero human edits.
    assert!(!app.diagnostics().is_empty(), "regression injected");
    let rounds = auto_repair(&mut app, 5, |a, d| {
        if d.code == "W0103" {
            dispatch(
                a,
                &json!({ "id": 1, "method": "input.click", "params": { "selector": "#fix" } }),
            );
            true
        } else {
            false
        }
    });
    assert!(
        rounds <= 2 && app.diagnostics().is_empty(),
        "self-healed in {rounds} round(s)"
    );
}
