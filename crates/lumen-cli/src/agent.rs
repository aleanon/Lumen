//! `lumen agent …` (C.5): the packaged client for the live-window endpoint
//! (03 §3) — one-shot calls (`agent call`) and an MCP stdio server
//! (`agent mcp`) that makes `lumen_agent::mcp_manifest` real by proxying
//! MCP `tools/call` onto the TCP line protocol.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

/// The endpoint address: `LUMEN_AGENT_ADDR`, else the discovery file a
/// `:0`-bound shell wrote (C.8a), else the fixed default.
fn discover_addr() -> String {
    if let Ok(addr) = std::env::var("LUMEN_AGENT_ADDR") {
        return addr;
    }
    let path = std::env::var("LUMEN_AGENT_ADDR_FILE")
        .unwrap_or_else(|_| "target/lumen-agent.addr".to_string());
    if let Ok(addr) = std::fs::read_to_string(path) {
        let addr = addr.trim();
        if !addr.is_empty() {
            return addr.to_string();
        }
    }
    "127.0.0.1:9230".to_string()
}

/// One JSON-RPC round-trip over the newline protocol. Attaches the bearer
/// token (`LUMEN_AGENT_TOKEN`) when set — required by non-loopback shells.
fn rpc_line(addr: &str, method: &str, params: Value) -> Result<Value, String> {
    let stream = TcpStream::connect(addr).map_err(|e| format!("connect {addr}: {e}"))?;
    let mut req = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    if let Ok(token) = std::env::var("LUMEN_AGENT_TOKEN") {
        req["auth"] = json!(token);
    }
    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    writeln!(writer, "{req}").map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&line).map_err(|e| format!("bad reply: {e}: {line}"))
}

/// `lumen agent call <method> [params-json]`.
pub fn cmd_call(method: Option<&str>, params: Option<&str>, json_out: bool) -> i32 {
    let Some(method) = method else {
        eprintln!("usage: lumen agent call <method> ['{{\"json\":\"params\"}}']");
        return 2;
    };
    let params: Value = match params {
        Some(p) => match serde_json::from_str(p) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("params must be JSON: {e}");
                return 2;
            }
        },
        None => json!({}),
    };
    match rpc_line(&discover_addr(), method, params) {
        Ok(reply) => {
            if json_out {
                println!("{reply}");
            } else if let Some(result) = reply.get("result") {
                println!("{}", serde_json::to_string_pretty(result).unwrap());
            } else {
                println!("{}", serde_json::to_string_pretty(&reply).unwrap());
            }
            i32::from(reply.get("error").is_some())
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

/// `lumen agent mcp`: an MCP stdio server (newline-delimited JSON-RPC, the
/// MCP stdio transport). Tools come from `lumen_agent::mcp_manifest`
/// (`ui_getTree` ↔ `ui.getTree`); `tools/call` proxies to the endpoint.
/// Point an MCP client at `lumen agent mcp` while `just run-agent` runs.
pub fn cmd_mcp() -> i32 {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(req) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let id = req.get("id").cloned();
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let reply = match method {
            "initialize" => Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "lumen-agent", "version": env!("CARGO_PKG_VERSION") },
            })),
            "tools/list" => {
                // The manifest's tools, each given the MCP-required input
                // schema (free-form object — the params documented in 03 §3).
                let mut tools = lumen_agent::mcp_manifest()["tools"].clone();
                if let Some(list) = tools.as_array_mut() {
                    for t in list {
                        t["inputSchema"] = json!({ "type": "object" });
                    }
                }
                Some(json!({ "tools": tools }))
            }
            "tools/call" => {
                let name = req["params"]["name"].as_str().unwrap_or("");
                let args = req["params"]
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let rpc_method = name.replace('_', ".");
                match rpc_line(&discover_addr(), &rpc_method, args) {
                    Ok(r) if r.get("result").is_some() => Some(json!({
                        "content": [{ "type": "text", "text": r["result"].to_string() }],
                    })),
                    Ok(r) => Some(json!({
                        "content": [{ "type": "text", "text": r["error"].to_string() }],
                        "isError": true,
                    })),
                    Err(e) => Some(json!({
                        "content": [{ "type": "text",
                            "text": format!("endpoint unreachable: {e} (is `just run-agent` up?)") }],
                        "isError": true,
                    })),
                }
            }
            // Notifications (no id) and unknown methods: MCP says ignore /
            // method-not-found respectively.
            _ if id.is_none() => None,
            other => {
                let resp = json!({ "jsonrpc": "2.0", "id": id,
                    "error": { "code": -32601, "message": format!("method not found: {other}") } });
                let _ = writeln!(out, "{resp}");
                let _ = out.flush();
                continue;
            }
        };
        if let (Some(id), Some(result)) = (id, reply) {
            let resp = json!({ "jsonrpc": "2.0", "id": id, "result": result });
            let _ = writeln!(out, "{resp}");
            let _ = out.flush();
        }
    }
    0
}
