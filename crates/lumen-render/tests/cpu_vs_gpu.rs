//! Phase **R0.2** — corpus-driven CPU↔GPU differential, with the capability
//! ratchet (`common::gpu_supported`).
//!
//! For every corpus scene whose capability is GPU-live today, the GPU output
//! must match the CPU reference within [`Tolerance::PARITY`]. As each R1
//! sub-phase lands a command class on the GPU, it flips that `Cap` to supported
//! and this test instantly enforces parity for it.
//!
//! **Skip policy:** unlike the legacy `gpu_parity` test this is *not* `#[ignore]`d
//! — it self-skips (returns early, logging) when no wgpu adapter is present, so
//! it runs automatically on a GPU box (this dev env: RTX 4070 / lavapipe) and in
//! GPU-CI, and no-ops cleanly on headless CI. Never silently passes a real
//! divergence.

mod common;

use common::*;
use lumen_render::cpu;
use lumen_render::gpu::GpuRenderer;

#[test]
fn gpu_matches_cpu_for_supported_capabilities() {
    let Some(gpu) = GpuRenderer::new() else {
        eprintln!("cpu_vs_gpu: no wgpu adapter; skipping (GPU-absent policy)");
        return;
    };

    let mut checked = 0usize;
    for s in corpus() {
        if !gpu_supported(s.cap) {
            eprintln!(
                "cpu_vs_gpu: {} ({:?}) not GPU-live yet — skipped",
                s.name, s.cap
            );
            continue;
        }
        let cpu_img = cpu::render(&s.dl, W, H, bg());
        let gpu_img = gpu.render(&s.dl, W, H, bg());
        assert_eq!(gpu_img.width(), W, "{} width", s.name);
        assert_eq!(gpu_img.height(), H, "{} height", s.name);
        let d = frame_diff(&cpu_img, &gpu_img);
        eprintln!(
            "cpu_vs_gpu: {} ({:?}) max ΔE {:.4}, {} px differ",
            s.name, s.cap, d.max_delta_e, d.differing
        );
        assert_frames_close(&cpu_img, &gpu_img, tolerance(s.cap), s.name);
        checked += 1;
    }

    assert!(
        checked >= 2,
        "expected to parity-check at least the rect+image scenes, checked {checked}"
    );
}
