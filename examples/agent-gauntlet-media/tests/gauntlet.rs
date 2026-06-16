//! M6-exit release gate: a media-rich, animated app (SVG + procedural video +
//! a shared-element hero + a rich-text editor) driven through lumen-agent, with
//! a 120fps (≤8.33ms/frame) budget assertion on the desktop CPU renderer.

use lumen_agent::dispatch;
use lumen_core::geometry::Size;
use lumen_widgets::Headless;
use serde_json::{json, Value};
use std::time::Instant;

fn rpc(app: &mut Headless, method: &str, params: Value) -> Value {
    dispatch(
        app,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }),
    )
}
fn tree(app: &mut Headless) -> String {
    rpc(app, "ui.getTree", json!({}))["result"].to_string()
}
fn count_images(app: &mut Headless) -> usize {
    fn walk(n: &lumen_core::semantics::SemanticsNode) -> usize {
        usize::from(n.role == lumen_core::semantics::Role::Image)
            + n.children.iter().map(walk).sum::<usize>()
    }
    walk(&app.semantics_doc().root.elided())
}

#[test]
fn m6_gauntlet_desktop() {
    let mut app = agent_gauntlet_media::main_app().run_headless(Size::new(400.0, 400.0));
    app.pump();

    // SVG logo + video frame + hero all render as images.
    assert!(count_images(&mut app) >= 3, "svg + video + hero render");
    assert!(tree(&mut app).contains("frame: 0"));

    // Step the video; the frame advances (the procedural decoder is re-clocked).
    rpc(&mut app, "input.click", json!({ "selector": "#next" }));
    assert!(tree(&mut app).contains("frame: 1"), "video stepped");

    // The rich-text editor accepts input and renders an emphasised run.
    rpc(
        &mut app,
        "input.type",
        json!({ "selector": "#notes", "text": " *more*" }),
    );
    assert!(tree(&mut app).contains("more"));

    // 120fps budget: a media-rich frame (SVG + video + layout + paint) stays
    // under 8.33 ms on the CPU reference renderer.
    let mut worst = 0.0f64;
    for _ in 0..30 {
        rpc(&mut app, "input.click", json!({ "selector": "#next" }));
        let t = Instant::now();
        app.pump();
        worst = worst.max(t.elapsed().as_secs_f64() * 1000.0);
    }
    // 120fps in release (the real claim); a relaxed bound in debug builds.
    let budget = if cfg!(debug_assertions) { 50.0 } else { 8.33 };
    assert!(
        worst < budget,
        "media frame budget: worst {worst:.2} ms (limit {budget})"
    );
}
