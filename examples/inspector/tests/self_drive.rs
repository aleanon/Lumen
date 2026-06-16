//! T4.4 acceptance: the inspector drives *itself* through `lumen-agent` — the
//! same protocol an external agent uses — navigating its panels and asserting
//! the result on its own semantic tree.

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
fn click(app: &mut Headless, selector: &str) {
    let r = rpc(app, "input.click", json!({ "selector": selector }));
    assert_eq!(r["result"]["ok"], json!(true), "click {selector}: {r}");
}

#[test]
fn inspector_drives_itself_via_agent() {
    let mut app = inspector::main_app().run_headless(Size::new(720.0, 520.0));
    app.pump();
    assert!(tree(&mut app).contains("Lumen Inspector"));

    // Tree panel (default): the sample semantic tree view is shown.
    let t = tree(&mut app);
    assert!(t.contains("Semantic tree") && t.contains("window"));

    // Style editor: bump font-size and see the preview update.
    click(&mut app, "tab:nth(2)");
    assert!(tree(&mut app).contains("preview: 16px"));
    click(&mut app, "#font-size-stepper #inc");
    assert!(
        tree(&mut app).contains("preview: 17px"),
        "style edit applied"
    );

    // Animation scrubber: drag the slider to its midpoint → frame ~50.
    click(&mut app, "tab:nth(3)");
    click(&mut app, "#scrub-slider");
    let scrubbed = tree(&mut app);
    assert!(scrubbed.contains("frame 50"), "scrubber moved: {scrubbed}");

    // Trace replay: step forward through the recorded trace.
    click(&mut app, "tab:nth(4)");
    assert!(tree(&mut app).contains("[1/3]"));
    click(&mut app, "#trace-next");
    assert!(tree(&mut app).contains("[2/3]"), "trace stepped");
}
