//! C.7 (docs/plan-remediation-2026-07.md): the tier-2/3 live orchestration —
//! a fresh component build swaps into the RUNNING host app in place (tier 2,
//! host state untouched), and an ABI-incompatible build downgrades to a
//! tier-3 snapshot restart that hands the state across.

use lumen_cli::dev::{Applied, Tier2Driver};
use lumen_core::events::{Event, PointerEvent};
use lumen_core::geometry::Size;
use lumen_widgets::center;
use std::path::PathBuf;
use std::process::Command;

fn target_debug() -> PathBuf {
    if let Ok(d) = std::env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(d).join("debug");
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join("debug")
}

fn dylib(name: &str) -> PathBuf {
    let file = if cfg!(target_os = "windows") {
        format!("{name}.dll")
    } else if cfg!(target_os = "macos") {
        format!("lib{name}.dylib")
    } else {
        format!("lib{name}.so")
    };
    target_debug().join(file)
}

fn ensure_fixtures() {
    if dylib("hot_a").exists() && dylib("hot_b").exists() && dylib("hot_c").exists() {
        return;
    }
    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", "hot_a", "-p", "hot_b", "-p", "hot_c"])
        .status()
        .expect("cargo build of fixtures");
    assert!(status.success(), "failed to build hot-patch fixtures");
}

#[test]
fn hot_swap_keeps_state_and_tier3_hands_it_across() {
    ensure_fixtures();
    let mut d = Tier2Driver::start(&dylib("hot_a"), Size::new(400.0, 300.0)).unwrap();
    let sem = d.app.semantics_json().to_string();
    assert!(sem.contains("Count"), "initial build renders: {sem}");

    // Host-owned state: click the counter twice.
    for _ in 0..2 {
        let p = center(d.app.node_bounds_by_id("count").unwrap());
        d.app.inject(Event::PointerDown(PointerEvent::at(p)));
        d.app.inject(Event::PointerUp(PointerEvent::at(p)));
        d.app.pump();
    }
    assert!(d.app.semantics_json().to_string().contains("count: 2"));

    // Tier 2: hot_b matches the host ABI — in-place swap, state untouched.
    let applied = d.apply_update(&dylib("hot_b")).unwrap();
    assert!(matches!(applied, Applied::Hot { .. }), "{applied:?}");
    let sem = d.app.semantics_json().to_string();
    assert!(sem.contains("Counter"), "swapped build renders: {sem}");
    assert!(
        sem.contains("count: 2"),
        "tier 2 preserved host state: {sem}"
    );

    // Tier 3: hot_c reports a foreign ABI — snapshot restart, state restored.
    let applied = d.apply_update(&dylib("hot_c")).unwrap();
    match &applied {
        Applied::Tier3 { dropped, .. } => assert_eq!(*dropped, 0, "clean handoff"),
        other => panic!("expected tier-3 downgrade, got {other:?}"),
    }
    let sem = d.app.semantics_json().to_string();
    assert!(
        !sem.contains("Counter"),
        "restarted build renders hot_c's label: {sem}"
    );
    assert!(
        sem.contains("count: 2"),
        "tier 3 handed the snapshot across: {sem}"
    );
}
