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
#![cfg(feature = "wgpu")]

mod common;

use common::*;
use lumen_render::cpu;
use lumen_render::gpu::Wgpu;

/// GX1: acquire an adapter, or decide whether absence is a skip or a failure.
///
/// The self-skip exists so this suite is harmless on adapter-less CI. But a
/// test that silently passes when it did nothing is worthless as a *gate*: the
/// GPU job's whole purpose is to prove these ran. `LUMEN_REQUIRE_GPU=1` turns
/// the absent-adapter branch into a failure, so a GPU runner that loses its
/// adapter reports a broken job instead of a green one.
fn require_gpu() -> Option<Wgpu> {
    match Wgpu::new() {
        Some(gpu) => Some(gpu),
        None if std::env::var_os("LUMEN_REQUIRE_GPU").is_some() => {
            panic!(
                "LUMEN_REQUIRE_GPU is set but no wgpu adapter was found. On CI \
                 this means the GPU job lost its driver (check that the \
                 lavapipe ICD is installed and VK_ICD_FILENAMES points at it); \
                 locally, unset LUMEN_REQUIRE_GPU to allow skipping."
            )
        }
        None => {
            eprintln!("cpu_vs_gpu: no wgpu adapter; skipping (GPU-absent policy)");
            None
        }
    }
}

/// Scenes a **specific backend** renders wrong, where Lumen is not at fault.
///
/// Measured 2026-08-08. `gradient_linear` renders as *nothing* on the **OpenGL**
/// backend: the draw is issued, bind group and instance data are identical to
/// the working case, and `textureSample` of the 512×1 Oklab ramp returns zeros.
/// Swapping that one call for `textureLoad` — same pipeline, same bind group,
/// same texture — renders it correctly, which localises the fault to GL's
/// filtered-sampling path. Three Lumen-side explanations were tested and
/// falsified: the sRGB format (the bilinear *image* scene filters an sRGB
/// texture on the same backend and is fine), the texture being one texel tall
/// (a two-row ramp fails identically), and the texture handle being dropped
/// after the bind group is built (keeping it alive changes nothing). wgpu
/// validation reports no error at any point.
///
/// **Not lavapipe.** The project recorded this for months as "the lavapipe job
/// fails on gradients", which is why R6 was filed as ungateable. On real
/// lavapipe the scene renders and matches the CPU better than any other
/// (ΔE 0.0039). The misattribution came from `VK_DRIVER_FILES` — it constrains
/// *Vulkan*, so with `Backends::all()` wgpu answered with the NVIDIA **GL**
/// adapter and the failure was pinned on the ICD that had just been swapped in.
/// `Wgpu::new` now prefers PRIMARY, so GL is only reached when it is all there
/// is — which is exactly when this entry applies.
///
/// Keyed on the reported backend, not the adapter name: vendor strings vary
/// by driver version, and this gap is a property of GL, not of a vendor.
///
/// Asserted, not skipped — same doctrine as `field_coverage.rs`'s `GPU_IGNORES`.
/// A skip would let this set grow silently, and would hide the good news: when
/// GL is fixed, this test fails and says to delete the entry.
const GL_GAPS: &[&str] = &["gradient_linear"];

fn is_driver_gap(gpu: &Wgpu, scene: &str) -> bool {
    gpu.adapter_is_gl() && GL_GAPS.contains(&scene)
}

#[test]
fn gpu_matches_cpu_for_opaque_and_renders_the_rest() {
    let Some(gpu) = require_gpu() else {
        return;
    };
    eprintln!("cpu_vs_gpu: adapter = {}", gpu.adapter_name());

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
            let drew = frame_diff(&gpu_img, &blank).differing > 0;
            if is_driver_gap(&gpu, s.name) {
                assert!(
                    !drew,
                    "`{}` is listed in GL_GAPS and this adapter ({}) is GL, but it now \
                     RENDERS. That is good news: the driver was fixed. Delete the \
                     entry so the scene is gated again.",
                    s.name,
                    gpu.adapter_name()
                );
                eprintln!(
                    "cpu_vs_gpu: {} — known driver gap on {}, not gated",
                    s.name,
                    gpu.adapter_name()
                );
                continue;
            }
            assert!(drew, "GPU rendered nothing for {}", s.name);
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
    let Some(gpu) = require_gpu() else {
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
        let drew = frame_diff(&gpu_img, &blank).differing > 0;
        if is_driver_gap(&gpu, s.name) {
            assert!(
                !drew,
                "`{}` is listed in GL_GAPS and {} is GL, but it now renders at 2×; \
                 delete the entry.",
                s.name,
                gpu.adapter_name()
            );
            continue;
        }
        assert!(drew, "GPU rendered nothing at 2× for {}", s.name);
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

/// `WgpuFallbackTinySkia` renders on any machine — the GPU when an adapter is
/// present, else the CPU fallback — and reports the active backend. Never blank.
#[test]
fn wgpu_fallback_renders_anywhere() {
    use lumen_render::{Renderer, WgpuFallbackTinySkia};
    let mut r = WgpuFallbackTinySkia::new();
    let scene = corpus()
        .into_iter()
        .find(|s| s.name == "rect_solid")
        .unwrap();
    let img = r.render_frame(&scene.dl, W, H, 1.0, bg());
    assert_eq!((img.width(), img.height()), (W, H));
    assert!(
        frame_diff(&img, &blank_frame()).differing > 0,
        "fallback renderer produced a blank frame"
    );
    let expected = if r.is_gpu() { "wgpu" } else { "tiny-skia" };
    assert_eq!(r.name(), expected);
    eprintln!("WgpuFallbackTinySkia active backend: {}", r.name());
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
