//! The dev-server file watcher (T1.7): watch `.lss` files and signal on change.
//!
//! The watcher runs notify on its own thread and sends a message on every file
//! event; the UI thread (which owns the non-`Send` app) reacts by re-reading the
//! file and applying a tier-1 reload. This keeps the app on one thread while
//! still reacting to on-disk edits within the watcher's latency.

use crate::proto::ReloadResult;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver};

/// Watch `path` for changes. Returns the watcher (keep it alive) and a receiver
/// that yields a unit each time the file changes.
pub fn watch_file(path: &Path) -> notify::Result<(RecommendedWatcher, Receiver<()>)> {
    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            if matches!(
                ev.kind,
                notify::EventKind::Modify(_) | notify::EventKind::Create(_)
            ) {
                let _ = tx.send(());
            }
        }
    })?;
    watcher.watch(path, RecursiveMode::NonRecursive)?;
    Ok((watcher, rx))
}

/// Compute the tier-1 reload result for a stylesheet `src` (parse only — the
/// app applies it). A parse error yields `status: "error"` with the diagnostics.
pub fn tier1_reload(src: &str) -> ReloadResult {
    let start = std::time::Instant::now();
    let (_sheet, diags) = lumen_style::parse("app.lss", src);
    let ok = !lumen_style::has_errors(&diags);
    ReloadResult {
        tier: 1,
        status: if ok { "ok" } else { "error" },
        duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        diagnostics: diags,
    }
}

// --- C.7: tier-2/3 live orchestration --------------------------------------

use crate::hotpatch::{HotComponent, Swap};
use lumen_core::geometry::Size;
use lumen_widgets::{col, widgets, App, Headless};
use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;
use std::time::Instant;

/// What applying a fresh component build did (C.7): the in-process form of
/// ADR-D2's `dylib_update` → `reload_result` exchange.
#[derive(Debug, PartialEq, Eq)]
pub enum Applied {
    /// Tier-2: swapped in place, host state untouched.
    Hot {
        /// The new component's build label.
        label: String,
        /// Swap latency (ms).
        ms: u64,
    },
    /// ABI mismatch → tier-3: fresh load + app restart with the snapshot
    /// handoff (ADR-D2's `restart_request` + `state_snapshot`, in-process).
    Tier3 {
        /// The new component's build label.
        label: String,
        /// Restart latency (ms).
        ms: u64,
        /// `W0002` values dropped by the lenient restore.
        dropped: usize,
    },
}

/// The live dev host (C.7): owns the app whose root renders the hot
/// component, watches for fresh builds, and applies them — tier 2 when the
/// ABI matches, tier-3 snapshot restart when it doesn't. State (signals,
/// focus, scroll) lives host-side, so tier 2 preserves it by construction
/// and tier 3 preserves it through the snapshot.
pub struct Tier2Driver {
    comp: Rc<RefCell<HotComponent>>,
    /// The hosted app (public: tests/tools drive and inspect it).
    pub app: Headless,
    size: Size,
}

impl Tier2Driver {
    /// Load the initial component and boot the host app around it.
    pub fn start(dylib: &std::path::Path, size: Size) -> Result<Tier2Driver, String> {
        let comp = Rc::new(RefCell::new(HotComponent::load(dylib)?));
        let mut app = Self::host_app(comp.clone()).run_headless(size);
        app.pump();
        Ok(Tier2Driver { comp, app, size })
    }

    /// The host view: the component's build output plus a host-owned counter
    /// signal that swap tiers must preserve.
    fn host_app(comp: Rc<RefCell<HotComponent>>) -> App {
        App::new(move |cx| {
            let n = cx.signal("host.counter", || 0i64);
            let label = comp.borrow().label().to_string();
            col![
                widgets::text(label).id("component"),
                widgets::button(format!("count: {}", n.get(cx.runtime())), move |rt| {
                    n.update(rt, |v| *v += 1)
                })
                .id("count")
            ]
        })
    }

    /// Apply a freshly built component: tier-2 swap, or tier-3 restart on an
    /// ABI mismatch.
    pub fn apply_update(&mut self, dylib: &std::path::Path) -> Result<Applied, String> {
        let t0 = Instant::now();
        let outcome = self.comp.borrow_mut().swap(dylib)?;
        match outcome {
            Swap::Patched(label) => {
                // The component changed outside the reactive store — force
                // the rebuild the same way stylesheet reload does.
                self.app.force_full_repaint();
                Ok(Applied::Hot {
                    label,
                    ms: t0.elapsed().as_millis() as u64,
                })
            }
            Swap::NeedsTier3 { .. } => {
                // restart_request: snapshot, reload the world, restore.
                let snap = self.app.snapshot();
                *self.comp.borrow_mut() = HotComponent::load(dylib)?;
                let label = self.comp.borrow().label().to_string();
                let (mut app, diags) =
                    Self::host_app(self.comp.clone()).run_headless_restored(self.size, snap);
                app.pump();
                self.app = app;
                Ok(Applied::Tier3 {
                    label,
                    ms: t0.elapsed().as_millis() as u64,
                    dropped: diags.len(),
                })
            }
        }
    }

    /// Rebuild `crate_name` (`cargo build -p`) and return the produced cdylib
    /// path — the "incremental cargo build" leg of the loop.
    pub fn build_component(crate_name: &str) -> Result<std::path::PathBuf, String> {
        let status = Command::new("cargo")
            .args(["build", "-p", crate_name])
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("cargo build -p {crate_name} failed"));
        }
        let file = if cfg!(target_os = "windows") {
            format!("{crate_name}.dll")
        } else if cfg!(target_os = "macos") {
            format!("lib{crate_name}.dylib")
        } else {
            format!("lib{crate_name}.so")
        };
        let dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into());
        Ok(std::path::PathBuf::from(dir).join("debug").join(file))
    }

    /// The full loop: watch `src_path`, rebuild `crate_name` on change, apply.
    /// Emits one `ReloadResult`-shaped JSON line per event; runs until the
    /// watcher drops. This is the `lumen dev` engine.
    pub fn watch_and_apply(
        &mut self,
        crate_name: &str,
        src_path: &std::path::Path,
    ) -> notify::Result<()> {
        let (_watcher, rx) = watch_file(src_path)?;
        while rx.recv().is_ok() {
            let line = match Self::build_component(crate_name).and_then(|p| self.apply_update(&p)) {
                Ok(Applied::Hot { label, ms }) => format!(
                    "{{\"tier\":2,\"status\":\"hot\",\"label\":{label:?},\"duration_ms\":{ms}}}"
                ),
                Ok(Applied::Tier3 { label, ms, dropped }) => format!(
                    "{{\"tier\":3,\"status\":\"restarted\",\"label\":{label:?},\
                     \"duration_ms\":{ms},\"dropped\":{dropped}}}"
                ),
                Err(e) => format!("{{\"status\":\"error\",\"message\":{e:?}}}"),
            };
            println!("{line}");
        }
        Ok(())
    }
}
