//! `lumen-agent` — the JSON-RPC 2.0 agent protocol over WebSocket (03 §3).
//!
//! Wraps a running app: observation (`ui.getTree`/`screenshot`/`getStyles`/
//! `getLayout`), actions (`input.click`/`type`/`key`/`scroll`), and diagnostics.
//! Synthesized input enters the *same* queue as OS input, so everything the
//! agent does is reproducible as a `lumen-test`. A sync WebSocket loop keeps the
//! (non-`Send`) app on the serving thread.
#![warn(missing_docs)]

use kurbo::Point;
use lumen_core::events::{Event, Key, KeyEvent, Modifiers, NamedKey, PointerEvent, TextInputEvent};
use lumen_core::semantics::{resolve_one, SemanticsNode};
use lumen_widgets::{center, Headless};
use serde_json::{json, Value};
use std::net::TcpListener;

mod base64;

/// Dispatch one JSON-RPC request against `app`, returning the JSON-RPC response.
pub fn dispatch(app: &mut Headless, req: &Value) -> Value {
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or_else(|| json!({}));

    match handle(app, method, &params) {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    }
}

type RpcResult = Result<Value, (i64, String)>;

fn handle(app: &mut Headless, method: &str, params: &Value) -> RpcResult {
    match method {
        "ui.getTree" => {
            let raw = params.get("raw").and_then(|v| v.as_bool()).unwrap_or(false);
            Ok(app.semantics_doc().to_json(raw))
        }
        "ui.getStyles" => Ok(app.get_styles(sel(params)?)),
        "ui.getLayout" => {
            let node = resolve(app, sel(params)?)?;
            let b = node.bounds;
            Ok(json!({ "bounds": { "x": b.x0, "y": b.y0, "w": b.width(), "h": b.height() } }))
        }
        "ui.screenshot" => {
            let annotate = params
                .get("annotate")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let img = app.screenshot();
            let mut out = json!({
                "image_base64": base64::encode(&img.to_png()),
                "width": img.width(),
                "height": img.height(),
            });
            if annotate {
                let root = app.semantics_doc().root.elided();
                let mut anns = Vec::new();
                collect_annotations(&root, &mut anns);
                out["annotations"] = Value::Array(anns);
            }
            Ok(out)
        }
        "app.diagnostics" => Ok(json!({ "diagnostics": [] })),
        "app.perf" => Ok(json!({
            "frame_ms_p50": 0.0, "frame_ms_p95": 0.0,
            "node_count": app.semantics_doc().root.elided().children.len(),
        })),
        "input.click" => {
            let node = resolve_action(app, params)?;
            let p = center(node.bounds);
            app.inject(Event::PointerDown(PointerEvent::at(p)));
            app.inject(Event::PointerUp(PointerEvent::at(p)));
            app.pump();
            Ok(json!({ "ok": true, "node": format!("node-{}", node.node) }))
        }
        "input.type" => {
            let node = resolve_action(app, params)?;
            let text = params
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or((-32602, "missing `text`".to_string()))?;
            let p = center(node.bounds);
            app.inject(Event::PointerDown(PointerEvent::at(p)));
            app.inject(Event::PointerUp(PointerEvent::at(p)));
            app.inject(Event::TextInput(TextInputEvent { text: text.into() }));
            app.pump();
            Ok(json!({ "ok": true }))
        }
        "input.key" => {
            let keys = params
                .get("keys")
                .and_then(|v| v.as_str())
                .ok_or((-32602, "missing `keys`".to_string()))?;
            app.inject(Event::KeyDown(key_event(keys)?));
            app.pump();
            Ok(json!({ "ok": true }))
        }
        "input.scroll" => {
            let dy = params.get("dy").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let p = resolve_action(app, params)
                .map(|n| center(n.bounds))
                .unwrap_or(Point::new(0.0, 0.0));
            app.inject(Event::Wheel(lumen_core::events::WheelEvent {
                pos: p,
                delta: kurbo::Vec2::new(0.0, dy),
                modifiers: Modifiers::empty(),
            }));
            app.pump();
            Ok(json!({ "ok": true }))
        }
        other => Err((-32601, format!("method not found: {other}"))),
    }
}

fn sel(params: &Value) -> Result<&str, (i64, String)> {
    params
        .get("selector")
        .and_then(|v| v.as_str())
        .ok_or((-32602, "missing `selector`".to_string()))
}

fn resolve(app: &Headless, selector: &str) -> Result<SemanticsNode, (i64, String)> {
    let root = app.semantics_doc().root.elided();
    match resolve_one(&root, selector) {
        Ok(id) => find_node(&root, id)
            .cloned()
            .ok_or((-32000, "node vanished".to_string())),
        Err(e) => Err((-32000, format!("{e:?}"))),
    }
}

fn resolve_action(app: &mut Headless, params: &Value) -> Result<SemanticsNode, (i64, String)> {
    app.pump();
    resolve(app, sel(params)?)
}

fn find_node(root: &SemanticsNode, id: u32) -> Option<&SemanticsNode> {
    if root.node == id {
        return Some(root);
    }
    root.children.iter().find_map(|c| find_node(c, id))
}

fn collect_annotations(n: &SemanticsNode, out: &mut Vec<Value>) {
    if !n.actions.is_empty() {
        out.push(json!({
            "node": format!("node-{}", n.node),
            "id": n.id.as_ref().map(|i| i.as_str()),
            "bounds": { "x": n.bounds.x0, "y": n.bounds.y0, "w": n.bounds.width(), "h": n.bounds.height() },
        }));
    }
    for c in &n.children {
        collect_annotations(c, out);
    }
}

fn key_event(keys: &str) -> Result<KeyEvent, (i64, String)> {
    let mut modifiers = Modifiers::empty();
    let parts: Vec<&str> = keys.split('+').collect();
    for m in &parts[..parts.len().saturating_sub(1)] {
        match *m {
            "Ctrl" => modifiers |= Modifiers::CTRL,
            "Shift" => modifiers |= Modifiers::SHIFT,
            "Alt" => modifiers |= Modifiers::ALT,
            "Meta" => modifiers |= Modifiers::META,
            _ => {}
        }
    }
    let last = *parts.last().unwrap_or(&"");
    let key = match last {
        "Tab" => Key::Named(NamedKey::Tab),
        "Enter" => Key::Named(NamedKey::Enter),
        "Space" => Key::Named(NamedKey::Space),
        "Escape" => Key::Named(NamedKey::Escape),
        "Backspace" => Key::Named(NamedKey::Backspace),
        s if s.chars().count() == 1 => Key::Character(s.into()),
        other => return Err((-32602, format!("unknown key `{other}`"))),
    };
    Ok(KeyEvent {
        key,
        modifiers,
        repeat: false,
    })
}

/// The MCP tool manifest: the agent methods as MCP tools (`.` → `_`), 03 §3.
pub fn mcp_manifest() -> Value {
    let tool = |name: &str, desc: &str| json!({ "name": name, "description": desc });
    json!({
        "tools": [
            tool("ui_getTree", "Get the semantic tree (accessibility/agent view)."),
            tool("ui_screenshot", "Capture a PNG screenshot, optionally ID-annotated."),
            tool("ui_getStyles", "Computed styles for a selector."),
            tool("ui_getLayout", "Layout bounds for a selector."),
            tool("input_click", "Click the node a selector resolves to."),
            tool("input_type", "Focus a node and type text."),
            tool("input_key", "Press a key chord."),
            tool("input_scroll", "Scroll a node."),
            tool("app_diagnostics", "Current diagnostics."),
        ]
    })
}

/// Serve the agent protocol on `listener` for one connection, driving `app`.
/// Blocking and single-threaded (the app lives here). Returns when the client
/// disconnects.
pub fn serve_one(listener: &TcpListener, app: &mut Headless) -> std::io::Result<()> {
    let (stream, _) = listener.accept()?;
    let mut ws = match tungstenite::accept(stream) {
        Ok(ws) => ws,
        Err(_) => return Ok(()),
    };
    loop {
        match ws.read() {
            Ok(tungstenite::Message::Text(txt)) => {
                let req: Value = serde_json::from_str(&txt).unwrap_or(Value::Null);
                let resp = dispatch(app, &req);
                if ws
                    .send(tungstenite::Message::Text(resp.to_string()))
                    .is_err()
                {
                    break;
                }
            }
            Ok(tungstenite::Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
    Ok(())
}
