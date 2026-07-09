//! `lumen-agent` — the JSON-RPC 2.0 agent protocol over WebSocket (03 §3).
//!
//! Wraps a running app: observation (`ui.getTree`/`screenshot`/`getStyles`/
//! `getLayout`), actions (`input.click`/`type`/`key`/`scroll`), and diagnostics.
//! Synthesized input enters the *same* queue as OS input, so everything the
//! agent does is reproducible as a `lumen-test`. A sync WebSocket loop keeps the
//! (non-`Send`) app on the serving thread.
#![warn(missing_docs)]

use kurbo::{Point, Rect};
use lumen_core::events::{Event, Key, KeyEvent, Modifiers, NamedKey, PointerEvent, TextInputEvent};
use lumen_core::semantics::{resolve_one, SemanticsNode};
use lumen_core::Color;
use lumen_widgets::{center, Headless, Renderer, Spawner};
use serde_json::{json, Value};
use std::net::TcpListener;

mod base64;

/// Dispatch one JSON-RPC request against `app`, returning the JSON-RPC response.
pub fn dispatch<R: Renderer, E: Spawner>(app: &mut Headless<R, E>, req: &Value) -> Value {
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

/// Autonomously repair an app (T7.5 AI-native): read its structured
/// diagnostics, apply `fixer` to each, and repeat until the app is clean or
/// `max_iters` is reached — the agent's detect → diagnose → fix → verify loop,
/// with no human in the loop. Returns the number of repair rounds taken.
pub fn auto_repair<R: Renderer, E: Spawner>(
    app: &mut Headless<R, E>,
    max_iters: usize,
    mut fixer: impl FnMut(&mut Headless<R, E>, &lumen_core::Diagnostic) -> bool,
) -> usize {
    for round in 0..max_iters {
        app.pump();
        let diags = app.diagnostics();
        if diags.is_empty() {
            return round;
        }
        let mut fixed_any = false;
        for d in &diags {
            if fixer(app, d) {
                fixed_any = true;
            }
        }
        if !fixed_any {
            return round; // nothing we know how to fix
        }
    }
    max_iters
}

/// Step recorded for export to a `lumen-test` regression suite.
enum Step {
    Click(String),
    Fill(String, String),
    Press(String, String),
    ExpectText(String, String),
    ExpectState(String, String),
}

/// A recording agent session (M2-exit): wraps [`dispatch`], remembers the
/// replayable input/assertion steps, and exports them as a standalone
/// `lumen-test` via `session.exportTest`. An agent connected only to
/// `lumen-agent` can thus turn an exploration into a committed regression test.
#[derive(Default)]
pub struct Session {
    steps: Vec<Step>,
}

impl Session {
    /// A new, empty session.
    pub fn new() -> Session {
        Session::default()
    }

    /// Number of recorded steps.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Whether nothing has been recorded.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Dispatch a JSON-RPC request, recording replayable steps. `session.*`
    /// methods are handled here; everything else delegates to [`dispatch`] and
    /// successful input methods are recorded.
    pub fn dispatch<R: Renderer, E: Spawner>(
        &mut self,
        app: &mut Headless<R, E>,
        req: &Value,
    ) -> Value {
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or_else(|| json!({}));

        if let Some(result) = self.handle_session(app, method, &params) {
            return match result {
                Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                Err((code, message)) => {
                    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
                }
            };
        }

        let resp = dispatch(app, req);
        if resp.get("result").is_some() {
            self.record(method, &params);
        }
        resp
    }

    fn record(&mut self, method: &str, params: &Value) {
        let sel = params
            .get("selector")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        match method {
            "input.click" => self.steps.push(Step::Click(sel)),
            "input.type" => {
                if let Some(t) = params.get("text").and_then(|v| v.as_str()) {
                    self.steps.push(Step::Fill(sel, t.to_string()));
                }
            }
            "input.key" => {
                if let Some(k) = params.get("keys").and_then(|v| v.as_str()) {
                    self.steps.push(Step::Press(sel, k.to_string()));
                }
            }
            _ => {}
        }
    }

    fn handle_session<R: Renderer, E: Spawner>(
        &mut self,
        app: &mut Headless<R, E>,
        method: &str,
        params: &Value,
    ) -> Option<RpcResult> {
        match method {
            "session.assertText" => Some(self.assert_text(app, params)),
            "session.assertState" => Some(self.assert_state(app, params)),
            "session.exportTest" => Some(self.export(params)),
            _ => None,
        }
    }

    fn assert_text<R: Renderer, E: Spawner>(
        &mut self,
        app: &mut Headless<R, E>,
        params: &Value,
    ) -> RpcResult {
        let selector = sel(params)?.to_string();
        let expected = params
            .get("equals")
            .and_then(|v| v.as_str())
            .ok_or((-32602, "missing `equals`".to_string()))?;
        app.pump();
        let node = resolve(app, &selector)?;
        if node.label == expected {
            self.steps
                .push(Step::ExpectText(selector, expected.to_string()));
            Ok(json!({ "ok": true }))
        } else {
            Err((
                -32001,
                format!("expected text {expected:?}, got {:?}", node.label),
            ))
        }
    }

    fn assert_state<R: Renderer, E: Spawner>(
        &mut self,
        app: &mut Headless<R, E>,
        params: &Value,
    ) -> RpcResult {
        let selector = sel(params)?.to_string();
        let state = params
            .get("state")
            .and_then(|v| v.as_str())
            .ok_or((-32602, "missing `state`".to_string()))?;
        app.pump();
        let node = resolve(app, &selector)?;
        if node.states.iter().any(|s| s.as_str() == state) {
            self.steps
                .push(Step::ExpectState(selector, state.to_string()));
            Ok(json!({ "ok": true }))
        } else {
            Err((-32001, format!("node lacks state {state:?}")))
        }
    }

    fn export(&self, params: &Value) -> RpcResult {
        let fn_name = params
            .get("fnName")
            .and_then(|v| v.as_str())
            .unwrap_or("agent_regression");
        let app_expr = params
            .get("appExpr")
            .and_then(|v| v.as_str())
            .ok_or((-32602, "missing `appExpr`".to_string()))?;
        let header = params.get("header").and_then(|v| v.as_str()).unwrap_or("");
        Ok(json!({ "source": export_test(fn_name, app_expr, header, &self.steps) }))
    }
}

/// Emit a standalone, `cargo test`-able `lumen-test` from recorded steps.
fn export_test(fn_name: &str, app_expr: &str, header: &str, steps: &[Step]) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    if !header.is_empty() {
        let _ = writeln!(s, "{header}");
    }
    let _ = writeln!(s, "#[test]");
    let _ = writeln!(s, "fn {fn_name}() {{");
    let _ = writeln!(s, "    lumen_test::block_on(async {{");
    let _ = writeln!(s, "        let app = lumen_test::TestApp::new({app_expr});");
    for step in steps {
        let line = match step {
            Step::Click(sel) => format!("        app.locator({sel:?}).click().await.unwrap();"),
            Step::Fill(sel, t) => {
                format!("        app.locator({sel:?}).fill({t:?}).await.unwrap();")
            }
            Step::Press(sel, k) => {
                format!("        app.locator({sel:?}).press({k:?}).await.unwrap();")
            }
            Step::ExpectText(sel, t) => format!(
                "        lumen_test::expect(app.locator({sel:?})).to_have_text({t:?}).await.unwrap();"
            ),
            Step::ExpectState(sel, st) => format!(
                "        lumen_test::expect(app.locator({sel:?})).to_have_state({st:?}).await.unwrap();"
            ),
        };
        let _ = writeln!(s, "{line}");
    }
    let _ = writeln!(s, "    }});");
    let _ = writeln!(s, "}}");
    s
}

fn handle<R: Renderer, E: Spawner>(
    app: &mut Headless<R, E>,
    method: &str,
    params: &Value,
) -> RpcResult {
    match method {
        "ui.getTree" => {
            let raw = params.get("raw").and_then(|v| v.as_bool()).unwrap_or(false);
            Ok(app.semantics_doc().to_json(raw))
        }
        "ui.getStyles" => Ok(app.get_styles(sel(params)?)),
        "ui.getDeps" => Ok(app.get_deps(sel(params)?)),
        "ui.whatDependsOn" => {
            let sig = params
                .get("signal")
                .and_then(|v| v.as_str())
                .ok_or((-32602, "missing `signal`".to_string()))?;
            Ok(app.what_depends_on(sig))
        }
        "ui.lastChange" => Ok(app.last_change()),
        "ui.getLayout" => {
            let node = resolve(app, sel(params)?)?;
            let b = node.bounds;
            let mut out = json!({
                "bounds": { "x": b.x0, "y": b.y0, "w": b.width(), "h": b.height() },
            });
            // Rendered ink bounds + whether content is clipped by its own box.
            if let Some(i) = node.ink {
                // Vertical overflow = real clipping (see audit::check_clipping).
                let over = (i.y1 - b.y1).max(b.y0 - i.y0);
                out["ink"] = json!({ "x": i.x0, "y": i.y0, "w": i.width(), "h": i.height() });
                out["clipped"] = json!(over > 0.5);
            }
            if let Some(tm) = node.text_metrics {
                out["text_metrics"] = json!({
                    "line_count": tm.line_count,
                    "box_height": tm.box_height,
                    "ascent": tm.ascent,
                    "descent": tm.descent,
                    "line_height": tm.line_height,
                    "content_height": tm.content_height,
                });
            }
            // Reactive dependencies if this node is a `cx.scope` root (F2): the
            // signals whose change re-runs this subtree.
            if let Some(deps) = &node.deps {
                out["deps"] = json!(deps);
            }
            Ok(out)
        }
        "ui.screenshot" => {
            // Zoomed, overlaid crop of one element (magnify a small defect).
            if let Some(s) = params.get("selector").and_then(|v| v.as_str()) {
                let node = resolve(app, s)?;
                let scale = params.get("scale").and_then(|v| v.as_f64()).unwrap_or(4.0);
                let overlay = params
                    .get("overlay")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let b = node.bounds;
                let m = 8.0; // margin around the box
                let region = Rect::new(b.x0 - m, b.y0 - m, b.x1 + m, b.y1 + m);
                let mut outlines = Vec::new();
                if overlay {
                    outlines.push((b, Color::srgb8(0x1a, 0x73, 0xe8, 0xff))); // box = blue
                    if let Some(i) = node.ink {
                        outlines.push((i, Color::srgb8(0xe8, 0x1a, 0x1a, 0xff)));
                        // ink = red
                    }
                }
                let img = app.screenshot_zoom(region, scale, &outlines);
                return Ok(json!({
                    "image_base64": base64::encode(&img.to_png()),
                    "width": img.width(),
                    "height": img.height(),
                    "box": { "x": b.x0, "y": b.y0, "w": b.width(), "h": b.height() },
                }));
            }
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
        "app.diagnostics" => Ok(json!({ "diagnostics": app.diagnostics() })),
        "ui.lint" => Ok(json!({
            "findings": app.lint().iter()
                .map(|d| json!({ "code": d.code, "message": d.message }))
                .collect::<Vec<_>>()
        })),
        "ui.probe" => {
            // Pixel color at (x, y) in physical screenshot px.
            let x = params.get("x").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let y = params.get("y").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let c = app.screenshot().pixel(x, y);
            Ok(json!({ "color": [c[0], c[1], c[2], c[3]] }))
        }
        "ui.probeRegion" => {
            // Uniform color of a w×h region at (x, y), or null if it varies.
            let x = params.get("x").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let y = params.get("y").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let w = params.get("w").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let h = params.get("h").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let uniform = app
                .screenshot()
                .region_is_uniform(x, y, w, h)
                .map(|c| json!([c[0], c[1], c[2], c[3]]));
            Ok(json!({ "uniform": uniform }))
        }
        "app.perf" => {
            // C.2: real values from the runtime's rolling painted-frame times.
            let (p50, p95, frames) = app.perf_stats();
            Ok(json!({
                "frame_ms_p50": p50,
                "frame_ms_p95": p95,
                "frames_rendered": frames,
                "node_count": app.semantics_doc().root.elided().children.len(),
            }))
        }
        "app.logs" => {
            // C.2: the runtime's diagnostic log ring. Page with `since` =
            // last seen seq + 1.
            let since = params.get("since").and_then(|v| v.as_u64()).unwrap_or(0);
            let entries: Vec<Value> = app
                .runtime()
                .logs_since(since)
                .into_iter()
                .map(|e| json!({ "seq": e.seq, "level": e.level, "message": e.message }))
                .collect();
            Ok(json!({ "entries": entries }))
        }
        "ui.waitFor" => {
            // C.1a: block until a node matching `selector` exists — and
            // optionally carries `state` / has label-or-value equal to
            // `text` — pumping between 10 ms polls so deferred task results
            // apply. The explicit wait for anything the actions' implicit
            // auto-wait doesn't cover. Not covered yet: clock-driven
            // animation settling (C.1b).
            let selector = sel(params)?.to_string();
            let want_state = params
                .get("state")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let want_text = params
                .get("text")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let timeout_ms = params
                .get("timeout_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(5000);
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
            loop {
                app.pump();
                let root = app.semantics_doc().root.elided();
                if let Ok(id) = resolve_one(&root, &selector) {
                    if let Some(n) = find_node(&root, id) {
                        let state_ok = want_state
                            .as_deref()
                            .is_none_or(|s| n.states.iter().any(|st| st.as_str() == s));
                        let text_ok = want_text
                            .as_deref()
                            .is_none_or(|t| n.label.trim() == t || n.value.as_deref() == Some(t));
                        if state_ok && text_ok {
                            return Ok(json!({
                                "ok": true,
                                "node": format!("node-{}", n.node),
                                "states": n.states.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                                "label": n.label,
                            }));
                        }
                    }
                }
                if std::time::Instant::now() >= deadline {
                    return Err((
                        -32000,
                        format!(
                            "Timeout({timeout_ms}ms) waiting for `{selector}`{}{}",
                            want_state
                                .map(|s| format!(" state={s}"))
                                .unwrap_or_default(),
                            want_text
                                .map(|t| format!(" text={t:?}"))
                                .unwrap_or_default()
                        ),
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        "input.click" => {
            let node = resolve_action(app, params)?;
            let p = center(node.bounds);
            app.inject(Event::PointerDown(PointerEvent::at(p)));
            app.inject(Event::PointerUp(PointerEvent::at(p)));
            app.pump();
            Ok(json!({ "ok": true, "node": format!("node-{}", node.node) }))
        }
        "input.invokeAction" => {
            // Geometry-free actuation: run the node's handler directly (F4.4).
            let selector = sel(params)?;
            let action = params
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("click");
            let id = app
                .invoke_action(selector, action)
                .map_err(|e| (-32602, e))?;
            Ok(json!({ "ok": true, "node": format!("node-{id}") }))
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
        // --- desktop system integration (T5.2) ------------------------------
        "input.drop" => {
            let node = resolve_action(app, params)?;
            let text = params
                .get("text")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let files = params
                .get("files")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|f| f.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            app.inject(Event::Drop(lumen_core::events::DropEvent {
                pos: center(node.bounds),
                data: lumen_core::events::DropData { text, files },
            }));
            app.pump();
            Ok(json!({ "ok": true }))
        }
        "clipboard.read" => Ok(json!({ "text": app.clipboard_read() })),
        "clipboard.write" => {
            let t = params.get("text").and_then(|v| v.as_str()).unwrap_or("");
            app.clipboard_write(t);
            Ok(json!({ "ok": true }))
        }
        "ui.getMenu" => Ok(json!({ "menu": app.menu() })),
        "menu.invoke" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or((-32602, "missing `id`".to_string()))?;
            match app.invoke_menu(id) {
                Some(label) => Ok(json!({ "ok": true, "label": label })),
                None => Err((-32000, format!("no enabled menu item `{id}`"))),
            }
        }
        "app.systemRequests" => Ok(json!({ "requests": app.system_requests() })),
        "ui.getWindows" => Ok(json!({ "windows": app.windows() })),
        "input.setLocale" => {
            let loc = params
                .get("locale")
                .and_then(|v| v.as_str())
                .unwrap_or("en");
            let rtl = lumen_widgets::i18n::Locale::new(loc).is_rtl();
            app.set_rtl(rtl);
            Ok(json!({ "ok": true, "locale": loc, "rtl": rtl }))
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

fn resolve<R: Renderer, E: Spawner>(
    app: &Headless<R, E>,
    selector: &str,
) -> Result<SemanticsNode, (i64, String)> {
    let root = app.semantics_doc().root.elided();
    match resolve_one(&root, selector) {
        Ok(id) => find_node(&root, id)
            .cloned()
            .ok_or((-32000, "node vanished".to_string())),
        Err(e) => Err((-32000, format!("{e:?}"))),
    }
}

/// C.1a auto-wait (05 §3, the live slice): before acting, poll every 10 ms —
/// pumping between polls so deferred task results apply — until the selector
/// resolves to exactly one *actionable* node (non-empty bounds, not
/// `disabled`), or `timeout_ms` (param; default 5000) elapses. `Ambiguous`
/// fails immediately with the candidates (05 §3 rule). Clock-driven animation
/// settling is NOT waited on yet (C.1b — the shell owns the wall→virtual
/// clock; see docs/plan-remediation-2026-07.md).
fn resolve_action<R: Renderer, E: Spawner>(
    app: &mut Headless<R, E>,
    params: &Value,
) -> Result<SemanticsNode, (i64, String)> {
    let selector = sel(params)?.to_string();
    let timeout_ms = params
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(5000);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        app.pump();
        let root = app.semantics_doc().root.elided();
        match resolve_one(&root, &selector) {
            Ok(id) => {
                if let Some(n) = find_node(&root, id) {
                    let actionable = n.bounds.width() > 0.0
                        && n.bounds.height() > 0.0
                        && !n.states.iter().any(|s| s.as_str() == "disabled");
                    if actionable {
                        return Ok(n.clone());
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err((
                            -32000,
                            format!(
                                "Timeout({timeout_ms}ms): `{selector}` resolved but is not \
                                 actionable (zero-size or disabled): {:?} {:?}",
                                n.bounds, n.states
                            ),
                        ));
                    }
                }
            }
            // Exactly-one is the contract: >1 matches can't be waited away.
            Err(e @ lumen_core::semantics::ResolveError::Ambiguous { .. }) => {
                return Err((-32000, format!("{e:?}")));
            }
            Err(e) => {
                if std::time::Instant::now() >= deadline {
                    return Err((-32000, format!("Timeout({timeout_ms}ms): {e:?}")));
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
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
        "Delete" => Key::Named(NamedKey::Delete),
        "ArrowUp" => Key::Named(NamedKey::ArrowUp),
        "ArrowDown" => Key::Named(NamedKey::ArrowDown),
        "ArrowLeft" => Key::Named(NamedKey::ArrowLeft),
        "ArrowRight" => Key::Named(NamedKey::ArrowRight),
        "Home" => Key::Named(NamedKey::Home),
        "End" => Key::Named(NamedKey::End),
        "PageUp" => Key::Named(NamedKey::PageUp),
        "PageDown" => Key::Named(NamedKey::PageDown),
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
            tool(
                "ui_getDeps",
                "Reactive signal dependencies of a selector (union + per-prop).",
            ),
            tool(
                "ui_whatDependsOn",
                "Predict which nodes update (patch vs rebuild) if a signal changes.",
            ),
            tool(
                "ui_lastChange",
                "What the last pump did: idle/patch/rebuild + patched nodes.",
            ),
            tool("input_click", "Click the node a selector resolves to."),
            tool(
                "input_invokeAction",
                "Activate a control by its handler (geometry-free): click/focus/dismiss.",
            ),
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
pub fn serve_one<R: Renderer, E: Spawner>(
    listener: &TcpListener,
    app: &mut Headless<R, E>,
) -> std::io::Result<()> {
    serve_one_session(listener, app, &mut Session::new())
}

/// Like [`serve_one`], but records the connection into `session` so it can be
/// exported as a regression suite (`session.exportTest`).
pub fn serve_one_session<R: Renderer, E: Spawner>(
    listener: &TcpListener,
    app: &mut Headless<R, E>,
    session: &mut Session,
) -> std::io::Result<()> {
    let (stream, _) = listener.accept()?;
    let mut ws = match tungstenite::accept(stream) {
        Ok(ws) => ws,
        Err(_) => return Ok(()),
    };
    loop {
        match ws.read() {
            Ok(tungstenite::Message::Text(txt)) => {
                let req: Value = serde_json::from_str(&txt).unwrap_or(Value::Null);
                let resp = session.dispatch(app, &req);
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
