//! Phase **R0.2** — corpus-driven CPU↔GPU differential.
//!
//! The GPU renders in **linear light** (sRGB target — the physically-correct
//! blend the live-window agent sees), while the deterministic `TinySkia`
//! reference blends in **gamma**. So the two agree *exactly* only on opaque,
//! non-AA, nearest-sampled content ([`exact_vs_cpu`]); anti-aliased / blended /
//! bilinear scenes diverge by design. This test therefore:
//!   - asserts tight CPU parity for the exact scenes (catches GPU regressions on
//!     opaque content), and
//!   - for the rest, logs the (expected) divergence and checks the GPU actually
//!     renders and is **deterministic** (same bytes across two renders).
//!
//! **Skip policy:** not `#[ignore]`d — self-skips (logging) when no wgpu adapter
//! is present, so it runs on a GPU box / GPU-CI and no-ops on headless CI.

mod common;

use common::*;
use lumen_render::cpu;
use lumen_render::gpu::Wgpu;

#[test]
fn gpu_matches_cpu_for_opaque_and_renders_the_rest() {
    let Some(gpu) = Wgpu::new() else {
        eprintln!("cpu_vs_gpu: no wgpu adapter; skipping (GPU-absent policy)");
        return;
    };

    let blank = blank_frame();
    let mut exact_checked = 0usize;
    for s in corpus() {
        let cpu_img = cpu::render(&s.dl, W, H, bg());
        let gpu_img = gpu.render(&s.dl, W, H, bg());
        assert_eq!(
            (gpu_img.width(), gpu_img.height()),
            (W, H),
            "{} size",
            s.name
        );

        let d = frame_diff(&cpu_img, &gpu_img);
        eprintln!(
            "cpu_vs_gpu: {} ({:?}) max ΔE {:.4}, {} px differ{}",
            s.name,
            s.cap,
            d.max_delta_e,
            d.differing,
            if exact_vs_cpu(s.name) {
                " [exact]"
            } else {
                " [linear≠gamma, informational]"
            }
        );

        if exact_vs_cpu(s.name) {
            assert_frames_close(&cpu_img, &gpu_img, Tolerance::PARITY, s.name);
            exact_checked += 1;
        } else {
            // Not parity-checked (linear vs gamma); but the GPU must render real
            // content and do so deterministically.
            assert!(
                frame_diff(&gpu_img, &blank).differing > 0,
                "GPU rendered nothing for {}",
                s.name
            );
            let again = gpu.render(&s.dl, W, H, bg());
            assert_frames_exact(&gpu_img, &again, &format!("{} GPU determinism", s.name));
        }
    }
    assert!(
        exact_checked >= 2,
        "expected ≥2 exact-parity scenes, checked {exact_checked}"
    );
}

/// R1.6: HiDPI. At 2× the GPU must render every scene at the scaled resolution;
/// the nearest-image path stays pixel-exact to the CPU (the SDF rects pick up a
/// sub-pixel AA band at non-unit scale that blends linear≠gamma, so only nearest
/// images are exact). Skips when no adapter.
#[test]
fn gpu_renders_at_2x_and_matches_cpu_for_nearest_images() {
    let Some(gpu) = Wgpu::new() else {
        eprintln!("gpu_matches_cpu_at_2x: no wgpu adapter; skipping");
        return;
    };
    let scale = 2.0;
    let (pw, ph) = (W * 2, H * 2);
    let blank = blank_2x();
    let mut exact_checked = 0usize;
    for s in corpus() {
        let gpu_img = gpu.render_at_scale(&s.dl, pw, ph, scale, bg());
        assert_eq!(
            (gpu_img.width(), gpu_img.height()),
            (pw, ph),
            "{} size",
            s.name
        );
        assert!(
            frame_diff(&gpu_img, &blank).differing > 0,
            "GPU rendered nothing at 2× for {}",
            s.name
        );
        // A nearest-sampled opaque image has no AA and no blend, so it's exact at
        // any scale — a clean geometry/scale parity check.
        if s.name == "image_checker" {
            let cpu_img = cpu::render_scaled(&s.dl, pw, ph, scale, bg());
            assert_frames_close(&cpu_img, &gpu_img, Tolerance::PARITY, s.name);
            exact_checked += 1;
        }
    }
    assert!(exact_checked >= 1, "expected the nearest-image scene at 2×");
}

fn blank_2x() -> lumen_render::RgbaImage {
    let px = bg().to_srgb8();
    let (pw, ph) = (W * 2, H * 2);
    let mut buf = Vec::with_capacity((pw * ph * 4) as usize);
    for _ in 0..(pw * ph) {
        buf.extend_from_slice(&px);
    }
    lumen_render::RgbaImage::from_raw(pw, ph, buf)
}
