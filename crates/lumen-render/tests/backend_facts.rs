//! O2.5: the active backend is reported, and a defective one says so (W0115).
//!
//! `Wgpu::new()` sweeps `Backends::PRIMARY` then `SECONDARY`. GL is reached
//! only when no Vulkan/Metal/DX12 adapter answers, and on GL `textureSample`
//! of the gradient ramp returns zeros — **every gradient in the frame renders
//! as nothing, with no validation error**. That is the defect class an agent
//! cannot otherwise see: nothing to read, a completely correct semantic tree,
//! and a screenshot that only looks wrong if you already know better.
//!
//! **Coverage limit, stated rather than implied.** These tests run on whatever
//! adapter this machine has. The GL arm cannot be forced — `WGPU_BACKEND=gl`
//! is overridden by the explicit `backends` field in the sweep — so what is
//! pinned here is the *invariant* (`W0115` is emitted iff the backend is GL,
//! once) rather than the GL branch specifically. On a non-GL box that proves
//! the quiet path; on a GL-only box the same assertions prove the loud one.
#![cfg(feature = "wgpu")]

use lumen_render::Renderer;

#[test]
fn the_advisory_matches_the_backend_and_fires_at_most_once() {
    let Some(mut g) = lumen_render::Wgpu::new() else {
        eprintln!("no wgpu adapter on this machine — skipping");
        return;
    };
    let is_gl = g.backend() == "Gl";
    assert_eq!(
        g.backend_has_known_defects(),
        is_gl,
        "today GL is the only backend with a known defect; adapter is {:?} ({})",
        g.adapter_name(),
        g.backend()
    );

    let first: Vec<&str> = g.take_diagnostics().iter().map(|d| d.code).collect();
    assert_eq!(
        first.contains(&"W0115"),
        is_gl,
        "W0115 must be emitted exactly when the backend is defective \
         (backend={}, adapter={:?})",
        g.backend(),
        g.adapter_name()
    );

    // A standing condition, not a per-frame event: reporting it every time
    // `lint()` runs would flush the 1000-entry ring in seconds.
    let second: Vec<&str> = g.take_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        !second.contains(&"W0115"),
        "the backend advisory must be latched, not repeated: {second:?}"
    );
}

#[test]
fn a_named_backend_is_always_reported() {
    let Some(g) = lumen_render::Wgpu::new() else {
        return;
    };
    assert_ne!(
        g.backend(),
        "unknown",
        "an adapter that answered must map to a named backend, or `app.perf` \
         cannot answer \"why is this slow\": {:?}",
        g.adapter_name()
    );
    assert!(g.is_gpu(), "the wgpu backend is GPU-backed by construction");
}
