//! C.5 (docs/plan-remediation-2026-07.md): `lumen agent call` speaks the
//! newline TCP protocol (with token attachment), and `lumen agent mcp`
//! serves MCP over stdio, proxying tools/call onto the endpoint.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};

fn lumen() -> &'static str {
    env!("CARGO_BIN_EXE_lumen")
}

/// A one-connection canned endpoint: asserts on the request, replies fixed.
fn canned_endpoint(
    expect_method: &'static str,
    reply: Value,
) -> (String, std::thread::JoinHandle<Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let handle = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        let req: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(req["method"], json!(expect_method), "{req}");
        let mut w = stream;
        writeln!(
            w,
            "{}",
            json!({ "jsonrpc": "2.0", "id": req["id"], "result": reply })
        )
        .unwrap();
        req
    });
    (addr, handle)
}

#[test]
fn agent_call_round_trips_with_token() {
    let (addr, server) = canned_endpoint("ui.lint", json!({ "findings": [] }));
    let out = Command::new(lumen())
        .args(["agent", "call", "ui.lint"])
        .env("LUMEN_AGENT_ADDR", &addr)
        .env("LUMEN_AGENT_TOKEN", "s3cret")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("findings"), "{stdout}");
    // The token rode along on the wire.
    let req = server.join().unwrap();
    assert_eq!(req["auth"], json!("s3cret"), "{req}");
}

#[test]
fn agent_mcp_serves_tools_and_proxies_calls() {
    let (addr, server) = canned_endpoint("ui.getTree", json!({ "root": { "role": "window" } }));
    let mut child = Command::new(lumen())
        .args(["agent", "mcp"])
        .env("LUMEN_AGENT_ADDR", &addr)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut ask = |req: Value| -> Value {
        writeln!(stdin, "{req}").unwrap();
        serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap()
    };

    let init = ask(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }));
    assert_eq!(init["result"]["serverInfo"]["name"], json!("lumen-agent"));

    let tools = ask(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    let list = tools["result"]["tools"].as_array().unwrap();
    assert!(
        list.iter().any(
            |t| t["name"] == json!("ui_getTree") && t["inputSchema"]["type"] == json!("object")
        ),
        "{tools}"
    );

    let call = ask(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": { "name": "ui_getTree", "arguments": {} } }));
    let text = call["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("window"), "{call}");
    assert!(call["result"].get("isError").is_none(), "{call}");

    drop(stdin);
    let _ = child.wait();
    server.join().unwrap();
}

#[test]
fn inspect_pretty_prints_the_tree() {
    // C.8b: `lumen inspect` renders role#id "label" [states] lines.
    let (addr, server) = canned_endpoint(
        "ui.getTree",
        json!({ "root": {
            "role": "window",
            "children": [
                { "role": "button", "id": "save", "label": "Save",
                  "states": ["disabled"], "children": [] }
            ]
        }}),
    );
    let out = Command::new(lumen())
        .args(["inspect"])
        .env("LUMEN_AGENT_ADDR", &addr)
        .output()
        .unwrap();
    server.join().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("window"), "{stdout}");
    assert!(
        stdout.contains("button#save \"Save\" [disabled]"),
        "{stdout}"
    );
}

#[test]
fn inspect_without_an_app_fails_readably() {
    let out = Command::new(lumen())
        .args(["inspect"])
        .env("LUMEN_AGENT_ADDR", "127.0.0.1:1") // nothing listens here
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("agent serve"), "points at the fix: {err}");
}
