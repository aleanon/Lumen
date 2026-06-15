//! `lumen-test` — Playwright-class headless testing for Lumen apps (05).
//!
//! Tests run the real app headless on the CPU reference renderer. Locators
//! resolve over the semantic tree (the same resolver the agent uses, 03 §2),
//! auto-wait per 05 §3, and synthesize input through the one input path.
//!
//! M0 seed: `TestApp`, `Locator` (click/fill/press/text), `expect`
//! (to_exist/to_have_text), virtual clock, and exact-golden `expect_screenshot`.
#![warn(missing_docs)]

use kurbo::{Rect, Size};
use lumen_core::events::{Event, Key, NamedKey, PointerEvent, TextInputEvent};
use lumen_core::semantics::{resolve_one, ResolveError, SemanticsNode};
use lumen_render::RgbaImage;
use lumen_widgets::{center, App, Headless};
use std::cell::RefCell;
use std::rc::Rc;

mod runtime;
pub use runtime::block_on;

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
}

impl TestApp {
    /// Run `app` headless at the default 800×600.
    pub fn new(app: App) -> TestApp {
        TestApp::with_size(app, Size::new(800.0, 600.0))
    }

    /// Run `app` headless at a specific size.
    pub fn with_size(app: App, size: Size) -> TestApp {
        // Resolve goldens relative to the crate under test. Cargo sets
        // CARGO_MANIFEST_DIR in the test process's environment at runtime, which
        // is the *calling* crate (not lumen-test); fall back to compile-time.
        let base = std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").to_string());
        TestApp {
            inner: Rc::new(RefCell::new(app.run_headless(size))),
            golden_dir: std::path::PathBuf::from(base).join("tests/golden/cpu"),
        }
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
            panic!("screenshot golden mismatch for {name}; wrote {actual:?}");
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

    /// Focus the node and type `text` (through the committed-text path).
    pub async fn fill(&self, text: &str) -> Result<(), LocatorError> {
        let id = self.wait_one().await?;
        let bounds = self.node_bounds(id).unwrap_or(Rect::ZERO);
        let mut h = self.inner.borrow_mut();
        h.inject(Event::PointerDown(PointerEvent::at(center(bounds))));
        h.inject(Event::PointerUp(PointerEvent::at(center(bounds))));
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
}

/// An assertion builder (auto-retrying, 05 §2).
pub struct Expect {
    locator: Locator,
}

/// Begin an assertion on `locator`.
pub fn expect(locator: Locator) -> Expect {
    Expect { locator }
}

impl Expect {
    /// Assert the locator resolves to a node (auto-waiting).
    pub async fn to_exist(&self) -> Result<(), LocatorError> {
        self.locator.wait_one().await.map(|_| ())
    }

    /// Assert the matched node's text equals `text` (auto-retrying).
    pub async fn to_have_text(&self, text: &str) -> Result<(), LocatorError> {
        let mut waited = 0.0;
        loop {
            if let Ok(got) = self.locator.text().await {
                if got == text {
                    return Ok(());
                }
            }
            if waited >= TIMEOUT_MS {
                return Err(LocatorError::Timeout);
            }
            {
                let mut h = self.locator.inner.borrow_mut();
                h.advance_clock(POLL_MS);
                h.pump();
            }
            waited += POLL_MS;
        }
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
