//! T2.3 acceptance: a tier-2 hot swap replaces a component's build() in place
//! in well under 2 s, the host-owned state survives, and an ABI-incompatible
//! component is rejected in favour of tier 3.

use lumen_cli::hotpatch::{HotComponent, Swap};
use lumen_core::Runtime;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

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

/// Ensure the fixture cdylibs exist (a real dev session would have just built
/// them; here we build on demand so the test is hermetic).
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
fn tier2_swap_preserves_state_and_is_fast() {
    ensure_fixtures();

    // Host-owned state store — this lives in the process, not the dylib.
    let rt = Runtime::new();
    let count = rt.signal("count", || 0i32);
    count.set(&rt, 5);

    let mut comp = HotComponent::load(&dylib("hot_a")).expect("load v1");
    assert_eq!(comp.label(), "Count");
    assert_eq!(comp.retired_count(), 0);

    // The "rebuild" already happened (hot_b is the edited build); time only the
    // libloading swap, which is what must land under 2 s on a warm cache.
    let t = Instant::now();
    let outcome = comp.swap(&dylib("hot_b")).expect("swap v2");
    let elapsed = t.elapsed();

    assert_eq!(outcome, Swap::Patched("Counter".to_string()));
    assert_eq!(comp.label(), "Counter");
    assert!(elapsed < Duration::from_secs(2), "swap took {elapsed:?}");
    assert_eq!(
        comp.retired_count(),
        1,
        "old library must be retired/leaked"
    );

    // State was untouched by the code swap.
    assert_eq!(count.get(&rt), 5);
}

#[test]
fn abi_mismatch_falls_back_to_tier3() {
    ensure_fixtures();

    let mut comp = HotComponent::load(&dylib("hot_a")).expect("load v1");
    let outcome = comp.swap(&dylib("hot_c")).expect("swap attempt");

    match outcome {
        Swap::NeedsTier3 { host, found } => {
            assert_ne!(host, found);
        }
        other => panic!("expected tier-3 fallback, got {other:?}"),
    }
    // The incompatible candidate must not have been adopted.
    assert_eq!(comp.label(), "Count");
    assert_eq!(comp.retired_count(), 0);
}
