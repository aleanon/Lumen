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
