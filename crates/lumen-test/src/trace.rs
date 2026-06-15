//! Test traces (05 §5): a JSONL event stream per test, plus failure artifacts
//! that embed the last screenshot + tree. Format `lumen-trace/1`.

use serde_json::{json, Map, Value};

/// Records trace events for one test and writes them as JSONL.
#[derive(Default)]
pub struct Tracer {
    events: Vec<Value>,
    seq: u64,
}

impl Tracer {
    /// A new, empty tracer.
    pub fn new() -> Tracer {
        Tracer::default()
    }

    fn record(&mut self, mut obj: Map<String, Value>) {
        self.seq += 1;
        obj.insert("schema".into(), json!("lumen-trace/1"));
        obj.insert("seq".into(), json!(self.seq));
        self.events.push(Value::Object(obj));
    }

    /// Record an input action (click/fill/…) on a selector.
    pub fn action(&mut self, action: &str, selector: &str) {
        let mut o = Map::new();
        o.insert("type".into(), json!("action"));
        o.insert("action".into(), json!(action));
        o.insert("selector".into(), json!(selector));
        self.record(o);
    }

    /// Record an assertion result.
    pub fn assertion(&mut self, name: &str, passed: bool) {
        let mut o = Map::new();
        o.insert("type".into(), json!("assert"));
        o.insert("name".into(), json!(name));
        o.insert("passed".into(), json!(passed));
        self.record(o);
    }

    /// Record a tree snapshot (the elided semantics doc).
    pub fn tree(&mut self, doc: Value) {
        let mut o = Map::new();
        o.insert("type".into(), json!("tree"));
        o.insert("tree".into(), doc);
        self.record(o);
    }

    /// Record a rendered frame with its damage rects.
    pub fn frame(&mut self, damage: Vec<[f64; 4]>) {
        let mut o = Map::new();
        o.insert("type".into(), json!("frame"));
        o.insert("damage".into(), json!(damage));
        self.record(o);
    }

    /// Record a failure, embedding the last screenshot (base64 PNG) + tree.
    pub fn failure(&mut self, message: &str, screenshot_png: &[u8], tree: Value) {
        let mut o = Map::new();
        o.insert("type".into(), json!("failure"));
        o.insert("message".into(), json!(message));
        o.insert("screenshot_base64".into(), json!(base64(screenshot_png)));
        o.insert("tree".into(), tree);
        self.record(o);
    }

    /// The trace as JSONL (one event per line).
    pub fn to_jsonl(&self) -> String {
        self.events
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The recorded events (for validation/inspection).
    pub fn events(&self) -> &[Value] {
        &self.events
    }

    /// Write the trace to `target/lumen-traces/<name>.trace.jsonl`; returns the
    /// path.
    pub fn write(&self, name: &str) -> std::path::PathBuf {
        let dir = std::path::PathBuf::from(
            std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string()),
        )
        .join("lumen-traces");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join(format!("{name}.trace.jsonl"));
        std::fs::write(&path, self.to_jsonl()).ok();
        path
    }
}

/// Minimal standard base64 (avoids a non-whitelisted crate).
fn base64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(A[((n >> 18) & 63) as usize] as char);
        out.push(A[((n >> 12) & 63) as usize] as char);
        out.push(if c.len() > 1 {
            A[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            A[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
