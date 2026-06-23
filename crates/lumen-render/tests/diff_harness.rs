//! Phase **R0.1** — self-tests for the differential harness in `common`.
//!
//! A regression harness is only worth trusting if it *fails* on a real
//! divergence. These tests prove the harness detects a one-pixel change, that
//! `assert_frames_exact` is strict, that `assert_frames_close` tolerates AA-scale
//! noise, and that the corpus + capability ratchet are internally consistent.

mod common;

use common::*;
use lumen_render::cpu;
use lumen_render::RgbaImage;

/// Render a flat field of one color.
fn solid(w: u32, h: u32, c: [u8; 4]) -> RgbaImage {
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..(w * h) {
        px.extend_from_slice(&c);
    }
    RgbaImage::from_raw(w, h, px)
}

#[test]
fn identical_frames_report_no_difference() {
    let a = solid(16, 16, [10, 20, 30, 255]);
    let b = a.clone();
    let r = frame_diff(&a, &b);
    assert_eq!(r.differing, 0);
    assert_eq!(r.max_channel_delta, 0);
    assert_eq!(r.max_delta_e, 0.0);
    assert_frames_exact(&a, &b, "identical");
    assert_frames_close(&a, &b, Tolerance::PARITY, "identical");
}

#[test]
fn detects_a_single_pixel_divergence() {
    let a = solid(16, 16, [10, 20, 30, 255]);
    let mut px = a.pixels().to_vec();
    // Perturb one pixel (index 100*4) by a large, clearly-visible amount.
    px[400] = 200;
    px[401] = 30;
    px[402] = 200;
    let b = RgbaImage::from_raw(16, 16, px);

    let r = frame_diff(&a, &b);
    assert_eq!(r.differing, 1, "exactly one pixel changed");
    assert!(r.max_channel_delta >= 170);
    assert!(
        r.max_delta_e > Tolerance::PARITY.max_delta_e,
        "a vivid change must exceed the parity ΔE ceiling (was {:.4})",
        r.max_delta_e
    );
    assert_eq!(count_over(&a, &b, Tolerance::PARITY.max_delta_e), 1);
}

#[test]
#[should_panic(expected = "must be byte-identical")]
fn exact_compare_rejects_one_pixel() {
    let a = solid(16, 16, [10, 20, 30, 255]);
    let mut px = a.pixels().to_vec();
    px[400] = px[400].wrapping_add(1); // a single 1-LSB change
    let b = RgbaImage::from_raw(16, 16, px);
    assert_frames_exact(&a, &b, "one-lsb"); // must panic
}

#[test]
fn close_compare_tolerates_subthreshold_noise() {
    // A uniform ±1 LSB across every channel is byte-different but perceptually
    // below the parity ceiling, so `close` accepts it while `exact` would not.
    let a = solid(32, 32, [128, 128, 128, 255]);
    let mut px = a.pixels().to_vec();
    for b in px.iter_mut() {
        *b = b.wrapping_add(1);
    }
    let b = RgbaImage::from_raw(32, 32, px);
    let r = frame_diff(&a, &b);
    assert!(r.differing > 0, "byte-different");
    assert!(
        r.max_delta_e <= Tolerance::PARITY.max_delta_e,
        "±1 LSB stays under ΔE 2.0 (was {:.3})",
        r.max_delta_e
    );
    assert_frames_close(&a, &b, Tolerance::PARITY, "subthreshold");
}

#[test]
fn corpus_is_nonempty_and_renders_on_cpu() {
    let scenes = corpus();
    assert!(scenes.len() >= 8, "corpus should cover every command class");
    for s in &scenes {
        let img = cpu::render(&s.dl, W, H, bg());
        assert_eq!(img.width(), W, "{} width", s.name);
        assert_eq!(img.height(), H, "{} height", s.name);
        // A scene must actually draw something (not the empty background).
        let blank = solid(W, H, bg().to_srgb8());
        assert!(
            frame_diff(&img, &blank).differing > 0,
            "scene {} painted nothing",
            s.name
        );
    }
}

#[test]
fn capability_ratchet_covers_the_live_subset() {
    // Capabilities the GPU backend matches today must be marked supported;
    // everything else stays unsupported until its R1 sub-phase flips it.
    assert!(gpu_supported(Cap::RectSolid));
    assert!(gpu_supported(Cap::Image));
    assert!(gpu_supported(Cap::RectRounded)); // R1.2
    assert!(gpu_supported(Cap::Path)); // R1.3
    for cap in [Cap::Gradient, Cap::Layer, Cap::Shader] {
        assert!(
            !gpu_supported(cap),
            "{cap:?} is not GPU-live yet; flip it in R1 when it matches CPU"
        );
    }
    // Every supported capability must have at least one corpus scene.
    for cap in [Cap::RectSolid, Cap::Image, Cap::RectRounded, Cap::Path] {
        assert!(
            corpus().iter().any(|s| s.cap == cap),
            "no corpus scene exercises {cap:?}"
        );
    }
}
