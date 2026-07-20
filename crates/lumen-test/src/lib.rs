//! `lumen-test` — Playwright-class headless testing for Lumen apps (05).
//!
//! Tests run the real app headless on the CPU reference renderer. Locators
//! resolve over the semantic tree (the same resolver the agent uses, 03 §2),
//! auto-wait per 05 §3, and synthesize input through the one input path.
//!
//! M0 seed: `TestApp`, `Locator` (click/fill/press/text), `expect`
//! (to_exist/to_have_text), virtual clock, and exact-golden `expect_screenshot`.
#![warn(missing_docs)]

use kurbo::Rect;
use lumen_core::events::{Event, Key, NamedKey, PointerEvent, TextInputEvent};
use lumen_core::semantics::{resolve_one, ResolveError, SemanticsNode};
use lumen_render::RgbaImage;
use lumen_widgets::{center, App, Headless};
use std::cell::RefCell;
use std::rc::Rc;

mod runtime;
pub mod session;
pub mod trace;
/// Re-exported so `#[lumen_test::test]` expansions (and test code) can name
/// the window size without importing kurbo.
pub use kurbo::Size;
/// The `#[lumen_test::test]` attribute (05 §1): async test body + per-test
/// size/scale/theme/app options over the `main_app()` convention.
pub use lumen_macros::test;
pub use runtime::block_on;
pub use session::Session;
pub use trace::Tracer;

/// Auto-wait poll step and timeout (05 §3).
const POLL_MS: f64 = 10.0;
const TIMEOUT_MS: f64 = 5000.0;

/// A locator/action failure (structured, 03 §2 / 05 §3).
#[derive(Clone, Debug, PartialEq)]
pub enum LocatorError {
    /// No node matched within the timeout. `nearest` are near-miss node ids.
    NotFound {
        /// Near-miss candidate node ids.
        nearest: Vec<u32>,
    },
    /// More than one node matched. `candidates` are their node ids.
    Ambiguous {
        /// Matching node ids.
        candidates: Vec<u32>,
    },
    /// The selector did not parse.
    Parse(String),
    /// Auto-wait timed out.
    Timeout,
}

/// A headless app under test.
pub struct TestApp {
    inner: Rc<RefCell<Headless>>,
    golden_dir: std::path::PathBuf,
    tracer: Rc<RefCell<Tracer>>,
}

/// Resolve the golden directory. `LUMEN_GOLDEN_DIR` overrides everything (used
/// when running tests on a device, where the assets are pushed to a known path);
/// otherwise goldens sit under the crate-under-test's `tests/golden/cpu`. Cargo
/// sets `CARGO_MANIFEST_DIR` at test runtime to the *calling* crate; the
/// compile-time value is the last-resort fallback.
fn golden_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("LUMEN_GOLDEN_DIR") {
        return std::path::PathBuf::from(dir);
    }
    let base = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").to_string());
    std::path::PathBuf::from(base).join("tests/golden/cpu")
}

impl TestApp {
    /// Invoke a named app command registered by the last build
    /// (`cx.register_command`) — the same registry the agent's
    /// `app.command` verb uses. Errors list the registered names (D9: 05 §2
    /// promised this; now it exists).
    pub fn run_command(&mut self, name: &str) -> Result<(), Vec<String>> {
        self.inner.borrow_mut().run_command(name)
    }

    /// Run `app` headless at the default 800×600.
    pub fn new(app: App) -> TestApp {
        TestApp::with_size(app, Size::new(800.0, 600.0))
    }

    /// Run `app` headless at a specific size.
    pub fn with_size(app: App, size: Size) -> TestApp {
        TestApp {
            inner: Rc::new(RefCell::new(app.run_headless(size))),
            golden_dir: golden_dir(),
            tracer: Rc::new(RefCell::new(Tracer::new())),
        }
    }

    /// Record an input action in the trace, with a tree snapshot (05 §5).
    pub fn trace_action(&self, action: &str, selector: &str) {
        let mut t = self.tracer.borrow_mut();
        t.action(action, selector);
        t.tree(self.inner.borrow().semantics_doc().to_json(false));
    }

    /// Record an assertion result in the trace.
    pub fn trace_assert(&self, name: &str, passed: bool) {
        self.tracer.borrow_mut().assertion(name, passed);
    }

    /// Write the trace to `target/lumen-traces/<name>.trace.jsonl`.
    pub fn write_trace(&self, name: &str) -> std::path::PathBuf {
        self.tracer.borrow().write(name)
    }

    /// The current trace events (for inspection/validation).
    pub fn trace_events(&self) -> Vec<serde_json::Value> {
        self.tracer.borrow().events().to_vec()
    }

    /// Record a failure artifact (last screenshot + tree) and write the trace.
    pub fn capture_failure(&self, name: &str, message: &str) -> std::path::PathBuf {
        let png = self.inner.borrow_mut().screenshot().to_png();
        let tree = self.inner.borrow().semantics_doc().to_json(false);
        self.tracer.borrow_mut().failure(message, &png, tree);
        self.write_trace(name)
    }

    /// Run `app` headless at `size` with theme `"light"|"dark"|"high-contrast"`
    /// (per-test config, 05 §1).
    pub fn with_options(app: App, size: Size, theme: &str) -> TestApp {
        TestApp::with_config(app, size, 1.0, theme)
    }

    /// Full per-test config (05 §1; the `#[lumen_test::test]` construction
    /// path): logical `size`, HiDPI `scale`, and theme.
    pub fn with_config(app: App, size: Size, scale: f64, theme: &str) -> TestApp {
        let t = TestApp::with_size(app, size);
        if (scale - 1.0).abs() > f64::EPSILON {
            t.inner.borrow_mut().set_scale(scale);
        }
        t.inner.borrow_mut().set_theme_str(theme);
        t
    }

    /// A locator for `selector` (grammar 03 §2).
    pub fn locator(&self, selector: &str) -> Locator {
        Locator {
            inner: self.inner.clone(),
            selector: selector.to_string(),
        }
    }

    /// Pump once, settling layout/effects.
    pub async fn pump_until_idle(&mut self) {
        self.inner.borrow_mut().pump();
    }

    /// The virtual clock.
    pub fn clock(&mut self) -> Clock<'_> {
        Clock { app: self }
    }

    /// The current frame.
    pub async fn screenshot(&mut self) -> RgbaImage {
        self.inner.borrow_mut().screenshot()
    }

    /// Exact-golden screenshot compare (05 §4). `LUMEN_UPDATE_GOLDENS=1` records.
    pub async fn expect_screenshot(&mut self, name: &str) {
        let img = self.inner.borrow_mut().screenshot();
        let path = self.golden_dir.join(format!("{name}.png"));
        if std::env::var_os("LUMEN_UPDATE_GOLDENS").is_some() {
            std::fs::create_dir_all(&self.golden_dir).unwrap();
            std::fs::write(&path, img.to_png()).unwrap();
            return;
        }
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|_| panic!("missing golden {path:?}; run with LUMEN_UPDATE_GOLDENS=1"));
        let expected = RgbaImage::from_png(&bytes).unwrap();
        if img != expected {
            let actual = path.with_extension("actual.png");
            std::fs::write(&actual, img.to_png()).unwrap();
            // T.3: a visual diff (differing pixels red over a dimmed base)
            // next to the golden + actual — Read all three to diagnose.
            let diff = path.with_extension("diff.png");
            if (img.width(), img.height()) == (expected.width(), expected.height()) {
                std::fs::write(
                    &diff,
                    lumen_render::diff::diff_image(&expected, &img).to_png(),
                )
                .unwrap();
            }
            panic!("screenshot golden mismatch for {name}; wrote {actual:?} and {diff:?}");
        }
    }

    /// Perceptual golden compare (05 §4, the GPU-renderer path): pass iff at
    /// most `tol.max_frac_over` of pixels exceed `tol.max_delta_e` (ΔE
    /// Oklab). Same record/`.actual.png`/`.diff.png` workflow as
    /// [`expect_screenshot`](Self::expect_screenshot); use
    /// `Tolerance::PARITY`/`::AA` from `lumen_render::diff`.
    pub async fn expect_screenshot_within(
        &mut self,
        name: &str,
        tol: lumen_render::diff::Tolerance,
    ) {
        let img = self.inner.borrow_mut().screenshot();
        let path = self.golden_dir.join(format!("{name}.png"));
        if std::env::var_os("LUMEN_UPDATE_GOLDENS").is_some() {
            std::fs::create_dir_all(&self.golden_dir).unwrap();
            std::fs::write(&path, img.to_png()).unwrap();
            return;
        }
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|_| panic!("missing golden {path:?}; run with LUMEN_UPDATE_GOLDENS=1"));
        let expected = RgbaImage::from_png(&bytes).unwrap();
        assert_eq!(
            (img.width(), img.height()),
            (expected.width(), expected.height()),
            "screenshot size mismatch for {name}"
        );
        let (ok, frac) = lumen_render::diff::frames_close(&expected, &img, tol);
        if !ok {
            let actual = path.with_extension("actual.png");
            std::fs::write(&actual, img.to_png()).unwrap();
            let diff = path.with_extension("diff.png");
            std::fs::write(
                &diff,
                lumen_render::diff::diff_image(&expected, &img).to_png(),
            )
            .unwrap();
            panic!(
                "perceptual golden mismatch for {name}: {:.3}% of pixels exceed \
                 ΔE {} (budget {:.3}%); wrote {actual:?} and {diff:?}",
                frac * 100.0,
                tol.max_delta_e,
                tol.max_frac_over * 100.0
            );
        }
    }

    /// The elided semantics root (typed).
    pub fn tree(&self) -> SemanticsNode {
        self.inner.borrow().semantics_doc().root.elided()
    }
}

/// Virtual-clock control.
pub struct Clock<'a> {
    app: &'a mut TestApp,
}

impl Clock<'_> {
    /// Advance virtual time by `ms` and pump.
    pub fn advance(&mut self, ms: f64) {
        let mut h = self.app.inner.borrow_mut();
        h.advance_clock(ms);
        h.pump();
    }
}

/// A locator over the semantic tree.
pub struct Locator {
    inner: Rc<RefCell<Headless>>,
    selector: String,
}

impl Locator {
    /// Number of matching nodes right now (no waiting).
    pub async fn count(&self) -> usize {
        let root = self.inner.borrow().semantics_doc().root.elided();
        match lumen_core::semantics::select(&root, &self.selector) {
            Ok(v) => v.len(),
            Err(_) => 0,
        }
    }

    /// The text (accessible name) of the single matched node.
    pub async fn text(&self) -> Result<String, LocatorError> {
        let id = self.wait_one().await?;
        let root = self.inner.borrow().semantics_doc().root.elided();
        Ok(find_node(&root, id)
            .map(|n| n.label.clone())
            .unwrap_or_default())
    }

    /// Click the single matched node (synthesizes pointer down/up at its center).
    pub async fn click(&self) -> Result<(), LocatorError> {
        let id = self.wait_one().await?;
        let bounds = self.node_bounds(id).unwrap_or(Rect::ZERO);
        let p = center(bounds);
        let mut h = self.inner.borrow_mut();
        h.inject(Event::PointerDown(PointerEvent::at(p)));
        h.inject(Event::PointerUp(PointerEvent::at(p)));
        h.pump();
        Ok(())
    }

    /// Right-click the single matched node (synthesizes a secondary-button
    /// pointer down/up at its center; T.2).
    pub async fn right_click(&self) -> Result<(), LocatorError> {
        let id = self.wait_one().await?;
        let bounds = self.node_bounds(id).unwrap_or(Rect::ZERO);
        let p = center(bounds);
        let mut down = PointerEvent::at(p);
        down.button = lumen_core::events::PointerButton::Right;
        let mut up = PointerEvent::at(p);
        up.button = lumen_core::events::PointerButton::Right;
        let mut h = self.inner.borrow_mut();
        h.inject(Event::PointerDown(down));
        h.inject(Event::PointerUp(up));
        h.pump();
        Ok(())
    }

    /// Focus the node and append `text` keystrokes (no clearing — the 05 §2
    /// counterpart to [`fill`](Self::fill); T.2).
    pub async fn type_text(&self, text: &str) -> Result<(), LocatorError> {
        let id = self.wait_one().await?;
        let bounds = self.node_bounds(id).unwrap_or(Rect::ZERO);
        let p = center(bounds);
        let mut h = self.inner.borrow_mut();
        h.inject(Event::PointerDown(PointerEvent::at(p)));
        h.inject(Event::PointerUp(PointerEvent::at(p)));
        h.inject(Event::TextInput(TextInputEvent {
            text: text.to_string(),
        }));
        h.pump();
        Ok(())
    }

    /// Focus the node and **replace** its content with `text` (05 §2):
    /// select-all (the editors' Ctrl+A binding), then the committed text
    /// lands over the selection. Use [`type_text`](Self::type_text) to
    /// append instead. (C.4a closed the earlier append-only divergence.)
    pub async fn fill(&self, text: &str) -> Result<(), LocatorError> {
        let id = self.wait_one().await?;
        let bounds = self.node_bounds(id).unwrap_or(Rect::ZERO);
        let mut h = self.inner.borrow_mut();
        h.inject(Event::PointerDown(PointerEvent::at(center(bounds))));
        h.inject(Event::PointerUp(PointerEvent::at(center(bounds))));
        h.inject(Event::KeyDown(lumen_core::events::KeyEvent {
            key: Key::Character("a".into()),
            modifiers: lumen_core::events::Modifiers::CTRL,
            repeat: false,
        }));
        h.inject(Event::TextInput(TextInputEvent {
            text: text.to_string(),
        }));
        h.pump();
        Ok(())
    }

    /// Press a named key on the focused node (e.g. `"Enter"`, `"Space"`, `"Tab"`).
    pub async fn press(&self, key: &str) -> Result<(), LocatorError> {
        let named = match key {
            "Enter" => NamedKey::Enter,
            "Space" => NamedKey::Space,
            "Tab" => NamedKey::Tab,
            "Escape" => NamedKey::Escape,
            _ => return Err(LocatorError::Parse(format!("unknown key {key}"))),
        };
        let mut h = self.inner.borrow_mut();
        h.inject(Event::KeyDown(lumen_core::events::KeyEvent {
            key: Key::Named(named),
            modifiers: lumen_core::events::Modifiers::empty(),
            repeat: false,
        }));
        h.pump();
        Ok(())
    }

    /// Auto-wait (05 §3): poll every 10 ms of virtual time until the selector
    /// resolves to exactly one node, or fail. `>1` fails `Ambiguous` immediately.
    async fn wait_one(&self) -> Result<u32, LocatorError> {
        let mut waited = 0.0;
        loop {
            let result = {
                let root = self.inner.borrow().semantics_doc().root.elided();
                resolve_one(&root, &self.selector)
            };
            match result {
                Ok(id) => return Ok(id),
                Err(ResolveError::Ambiguous { candidates }) => {
                    return Err(LocatorError::Ambiguous { candidates })
                }
                Err(ResolveError::Parse(p)) => return Err(LocatorError::Parse(p)),
                Err(ResolveError::NotFound { .. }) => {
                    if waited >= TIMEOUT_MS {
                        let root = self.inner.borrow().semantics_doc().root.elided();
                        let nearest = match resolve_one(&root, &self.selector) {
                            Err(ResolveError::NotFound { nearest }) => nearest,
                            _ => Vec::new(),
                        };
                        return Err(LocatorError::NotFound { nearest });
                    }
                    let mut h = self.inner.borrow_mut();
                    h.advance_clock(POLL_MS);
                    h.pump();
                    waited += POLL_MS;
                }
            }
        }
    }

    fn node_bounds(&self, id: u32) -> Option<Rect> {
        let root = self.inner.borrow().semantics_doc().root.elided();
        find_node(&root, id).map(|n| n.bounds)
    }

    /// The matched node's window-space bounds (05 §2).
    pub async fn bounds(&self) -> Result<Rect, LocatorError> {
        let id = self.wait_one().await?;
        Ok(self.node_bounds(id).unwrap_or(Rect::ZERO))
    }

    /// The matched node's value (inputs/sliders).
    pub async fn value(&self) -> Result<Option<String>, LocatorError> {
        let id = self.wait_one().await?;
        let root = self.inner.borrow().semantics_doc().root.elided();
        Ok(find_node(&root, id).and_then(|n| n.value.clone()))
    }

    /// The matched node's active semantic states.
    pub async fn states(&self) -> Result<Vec<String>, LocatorError> {
        let id = self.wait_one().await?;
        let root = self.inner.borrow().semantics_doc().root.elided();
        Ok(find_node(&root, id)
            .map(|n| n.states.iter().map(|s| s.as_str().to_string()).collect())
            .unwrap_or_default())
    }

    /// A computed style property of the matched node (canonical form, 04 §7).
    pub async fn style(&self, prop: &str) -> serde_json::Value {
        let styles = self.inner.borrow().get_styles(&self.selector);
        styles.get(prop).cloned().unwrap_or(serde_json::Value::Null)
    }

    /// Hover the matched node (synthesizes a pointer move to its center).
    pub async fn hover(&self) -> Result<(), LocatorError> {
        let id = self.wait_one().await?;
        let p = center(self.node_bounds(id).unwrap_or(Rect::ZERO));
        let mut h = self.inner.borrow_mut();
        h.inject(Event::PointerMove(PointerEvent::at(p)));
        h.pump();
        Ok(())
    }

    /// Focus the matched node (clicks it).
    pub async fn focus(&self) -> Result<(), LocatorError> {
        self.click().await
    }

    /// Double-click the matched node.
    pub async fn dblclick(&self) -> Result<(), LocatorError> {
        self.click().await?;
        self.click().await
    }

    /// Drag from the matched node to `target` (pointer down → move → up).
    pub async fn drag_to(&self, target: &Locator) -> Result<(), LocatorError> {
        let from = center(
            self.node_bounds(self.wait_one().await?)
                .unwrap_or(Rect::ZERO),
        );
        let to = center(
            target
                .node_bounds(target.wait_one().await?)
                .unwrap_or(Rect::ZERO),
        );
        let mut h = self.inner.borrow_mut();
        h.inject(Event::PointerDown(PointerEvent::at(from)));
        h.inject(Event::PointerMove(PointerEvent::at(to)));
        h.inject(Event::PointerUp(PointerEvent::at(to)));
        h.pump();
        Ok(())
    }

    /// Set the matched node's value by dragging to `fraction` of its width
    /// (sliders). `fraction` is in `0.0..=1.0`.
    pub async fn set_value(&self, fraction: f64) -> Result<(), LocatorError> {
        let b = self
            .node_bounds(self.wait_one().await?)
            .unwrap_or(Rect::ZERO);
        let p = kurbo::Point::new(
            b.x0 + fraction.clamp(0.0, 1.0) * b.width(),
            b.y0 + b.height() / 2.0,
        );
        let mut h = self.inner.borrow_mut();
        h.inject(Event::PointerDown(PointerEvent::at(p)));
        h.inject(Event::PointerUp(PointerEvent::at(p)));
        h.pump();
        Ok(())
    }
}

/// An assertion builder (auto-retrying, 05 §2).
pub struct Expect {
    locator: Locator,
}

/// Begin an assertion on `locator`.
pub fn expect(locator: Locator) -> Expect {
    Expect { locator }
}

/// Poll `$cond` (an expression evaluating to `Result<bool, LocatorError>`)
/// on the virtual clock until it passes or the 05 §3 budget elapses — so
/// **every** assertion auto-retries (T.2). A `NotFound` keeps waiting (the
/// node may appear); `Ambiguous`/`Parse` fail fast (waiting can't fix them).
macro_rules! retrying {
    ($self:ident, $cond:expr) => {{
        let mut waited = 0.0;
        loop {
            match $cond {
                Ok(true) => return Ok(()),
                Ok(false) | Err(LocatorError::NotFound { .. }) | Err(LocatorError::Timeout) => {}
                Err(e) => return Err(e),
            }
            if waited >= TIMEOUT_MS {
                return Err(LocatorError::Timeout);
            }
            {
                let mut h = $self.locator.inner.borrow_mut();
                h.advance_clock(POLL_MS);
                h.pump();
            }
            waited += POLL_MS;
        }
    }};
}

impl Expect {
    /// Assert the locator resolves to a node (auto-waiting).
    pub async fn to_exist(&self) -> Result<(), LocatorError> {
        self.locator.wait_one().await.map(|_| ())
    }

    /// Assert the matched node is visible: resolved with non-empty bounds
    /// (auto-retrying).
    pub async fn to_be_visible(&self) -> Result<(), LocatorError> {
        retrying!(self, {
            self.locator
                .bounds()
                .await
                .map(|b| b.width() > 0.0 && b.height() > 0.0)
        })
    }

    /// Assert the matched node's text equals `text` (auto-retrying).
    pub async fn to_have_text(&self, text: &str) -> Result<(), LocatorError> {
        retrying!(self, self.locator.text().await.map(|got| got == text))
    }

    /// Assert the matched node contains `text` (substring, auto-retrying).
    pub async fn to_contain_text(&self, text: &str) -> Result<(), LocatorError> {
        retrying!(self, {
            self.locator.text().await.map(|got| got.contains(text))
        })
    }

    /// Assert the matched node's value equals `value` (auto-retrying).
    pub async fn to_have_value(&self, value: &str) -> Result<(), LocatorError> {
        retrying!(self, {
            self.locator
                .value()
                .await
                .map(|v| v.as_deref() == Some(value))
        })
    }

    /// Assert the matched node has state `state` (e.g. `"checked"`,
    /// `"focused"`; auto-retrying).
    pub async fn to_have_state(&self, state: &str) -> Result<(), LocatorError> {
        retrying!(self, {
            self.locator
                .states()
                .await
                .map(|s| s.iter().any(|s| s == state))
        })
    }

    /// Assert the matched node is focused (auto-retrying).
    pub async fn to_be_focused(&self) -> Result<(), LocatorError> {
        self.to_have_state("focused").await
    }

    /// Assert the matched node is disabled (auto-retrying).
    pub async fn to_be_disabled(&self) -> Result<(), LocatorError> {
        self.to_have_state("disabled").await
    }

    /// Assert the selector resolves to exactly `n` nodes (auto-retrying).
    pub async fn to_have_count(&self, n: usize) -> Result<(), LocatorError> {
        retrying!(self, {
            Ok::<bool, LocatorError>(self.locator.count().await == n)
        })
    }

    /// Assert a computed style property equals the canonical `value` JSON
    /// (auto-retrying).
    pub async fn to_have_style(
        &self,
        prop: &str,
        value: serde_json::Value,
    ) -> Result<(), LocatorError> {
        retrying!(self, {
            let got = self.locator.style(prop).await;
            Ok::<bool, LocatorError>(got.get("value") == Some(&value) || got == value)
        })
    }

    /// Assert the matched node's bounds are within `tol` of `expected`
    /// (auto-retrying).
    pub async fn to_have_bounds_within(
        &self,
        expected: Rect,
        tol: f64,
    ) -> Result<(), LocatorError> {
        retrying!(self, {
            self.locator.bounds().await.map(|b| {
                (b.x0 - expected.x0).abs() <= tol
                    && (b.y0 - expected.y0).abs() <= tol
                    && (b.width() - expected.width()).abs() <= tol
                    && (b.height() - expected.height()).abs() <= tol
            })
        })
    }
}

fn find_node(root: &SemanticsNode, id: u32) -> Option<&SemanticsNode> {
    if root.node == id {
        return Some(root);
    }
    for c in &root.children {
        if let Some(n) = find_node(c, id) {
            return Some(n);
        }
    }
    None
}
