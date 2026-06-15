//! T1.8 acceptance: drive the counter app end-to-end over a real WebSocket
//! socket via the JSON-RPC protocol (transcript assertions tolerant of timing).

use lumen_agent::serve_one;
use lumen_widgets::{widgets, App};
use serde_json::{json, Value};
use std::net::TcpListener;
use tungstenite::Message;

fn counter() -> App {
    App::new(|cx| {
        let count = cx.signal("count", || 0i32);
        let v = count.get(cx.runtime());
        widgets::column(vec![
            widgets::text(format!("Count: {v}")).id("count"),
            widgets::button("+1", move |rt| count.update(rt, |c| *c += 1)).id("increment"),
        ])
    })
}

fn call<S: std::io::Read + std::io::Write>(
    ws: &mut tungstenite::WebSocket<S>,
    method: &str,
    params: Value,
) -> Value {
    let req = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    ws.send(Message::Text(req.to_string())).unwrap();
    loop {
        if let Message::Text(t) = ws.read().unwrap() {
            return serde_json::from_str(&t).unwrap();
        }
    }
}

#[test]
fn agent_drives_counter_over_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let server = std::thread::spawn(move || {
        let mut app = counter().run_headless(lumen_core::geometry::Size::new(400.0, 200.0));
        app.pump();
        serve_one(&listener, &mut app).unwrap();
    });

    let (mut ws, _) = tungstenite::connect(format!("ws://127.0.0.1:{port}/agent")).unwrap();

    // Observe: the tree shows "Count: 0".
    let tree = call(&mut ws, "ui.getTree", json!({}));
    assert_eq!(tree["jsonrpc"], "2.0");
    assert!(tree["result"].to_string().contains("Count: 0"));

    // Act: click #increment.
    let clicked = call(&mut ws, "input.click", json!({ "selector": "#increment" }));
    assert_eq!(clicked["result"]["ok"], json!(true));
    assert_eq!(clicked["result"]["node"], json!("node-2"));

    // Observe: the label updated to "Count: 1".
    let tree = call(&mut ws, "ui.getTree", json!({}));
    assert!(
        tree["result"].to_string().contains("Count: 1"),
        "tree: {}",
        tree["result"]
    );

    // Screenshot returns a base64 PNG of the right size.
    let shot = call(&mut ws, "ui.screenshot", json!({ "annotate": true }));
    assert_eq!(shot["result"]["width"], json!(400));
    assert!(shot["result"]["image_base64"].as_str().unwrap().len() > 100);
    // annotations include the interactive button
    assert!(shot["result"]["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["id"] == json!("increment")));

    // getLayout returns bounds for a selector.
    let layout = call(&mut ws, "ui.getLayout", json!({ "selector": "#count" }));
    assert!(layout["result"]["bounds"]["w"].as_f64().unwrap() > 0.0);

    // Unknown method -> JSON-RPC method-not-found.
    let err = call(&mut ws, "no.such.method", json!({}));
    assert_eq!(err["error"]["code"], json!(-32601));

    ws.close(None).unwrap();
    let _ = ws.read();
    server.join().unwrap();
}

#[test]
fn mcp_manifest_lists_tools() {
    let m = lumen_agent::mcp_manifest();
    let tools = m["tools"].as_array().unwrap();
    assert!(tools.iter().any(|t| t["name"] == json!("ui_getTree")));
    assert!(tools.iter().any(|t| t["name"] == json!("input_click")));
}
