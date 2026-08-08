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

    // ID1: assert the ROUND-TRIP property, not a literal handle. This used to
    // pin `"node-2"`, which was really asserting an arena slot number — it
    // would have broken on any tree reshuffle, and pinning `"nx-<hex>"`
    // instead would just re-create that brittleness in a new spelling. What
    // actually matters is that a handle the server returns identifies the same
    // node when handed straight back.
    let handle = clicked["result"]["node"].as_str().unwrap().to_string();
    assert!(handle.starts_with("nx-"), "{clicked}");
    let again = call(
        &mut ws,
        "ui.getLayout",
        json!({ "selector": handle.clone() }),
    );
    assert!(
        again.get("error").is_none(),
        "a returned handle must resolve back as a selector: {again}"
    );

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

/// ID0: an agent connecting to an unknown build must be able to ask what it is
/// talking to, rather than pattern-matching handle strings and guessing.
#[test]
fn agent_protocol_reports_versions_and_deprecations() {
    let mut h = counter().run_headless(kurbo::Size::new(300.0, 200.0));
    h.pump();

    let r = lumen_agent::dispatch(
        &mut h,
        &json!({"jsonrpc":"2.0","id":1,"method":"agent.protocol"}),
    );
    let p = &r["result"];

    assert_eq!(
        p["semantics"],
        lumen_core::semantics::SCHEMA,
        "must report the same schema string ui.getTree stamps, not a literal"
    );
    assert!(p["rpc"].is_string(), "rpc version present");

    // `accepts` is separate from `semantics` on purpose: during the ID2 alias
    // window the server emits only the new handle form while still accepting
    // the old one, and no version number can express that.
    let accepts = p["accepts"]["nodeHandles"]
        .as_array()
        .expect("accepted handle forms listed");
    assert!(
        accepts.iter().any(|v| v == "nx-<hex>"),
        "the replacement form must be advertised: {accepts:?}"
    );

    let dep = &p["deprecations"][0];
    assert_eq!(dep["what"], "node-<index>");
    assert_eq!(dep["replacement"], "nx-<hex>");
    assert_eq!(
        dep["code"],
        lumen_core::codes::W0302,
        "deprecation must name the diagnostic an agent will actually receive"
    );
}
