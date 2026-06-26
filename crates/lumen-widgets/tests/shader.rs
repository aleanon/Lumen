//! T4.1 acceptance: 3 sample shaders match GPU goldens (perceptual); a broken
//! shader edit keeps the previous frame and reports E0201; CPU fallback fills.
#![cfg(feature = "wgpu")]

use lumen_core::Color;
use lumen_render::RgbaImage;
use lumen_widgets::shader::ShaderWidget;
use std::path::PathBuf;

const W: u32 = 64;
const H: u32 = 64;

// Three sample fragment shaders.
const GRADIENT: &str = "@fragment fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> { return vec4<f32>(uv.x, uv.y, 0.5, 1.0); }";
const SOLID: &str = "@fragment fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> { return vec4<f32>(u.params.rgb, 1.0); }";
const RINGS: &str = "@fragment fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> { let d = distance(uv, vec2<f32>(0.5, 0.5)); let v = 0.5 + 0.5 * sin(d * 40.0); return vec4<f32>(v, v * 0.6, 1.0 - v, 1.0); }";
const BROKEN: &str = "@fragment fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> { return not_a_function(uv); }";

fn golden(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/gpu")
        .join(format!("{name}.png"))
}

/// Perceptual golden compare (ΔE Oklab ≤ 2.0 on ≤ 0.1% of pixels, 05 §4).
fn check_golden(name: &str, img: &RgbaImage) {
    let path = golden(name);
    if std::env::var_os("LUMEN_UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, img.to_png()).unwrap();
        return;
    }
    let expected = RgbaImage::from_png(
        &std::fs::read(&path).unwrap_or_else(|_| panic!("missing golden {path:?}")),
    )
    .unwrap();
    let mut bad = 0usize;
    for (a, b) in img
        .pixels()
        .chunks_exact(4)
        .zip(expected.pixels().chunks_exact(4))
    {
        let de = Color::srgb8(a[0], a[1], a[2], a[3])
            .delta_e_oklab(Color::srgb8(b[0], b[1], b[2], b[3]));
        if de > 2.0 {
            bad += 1;
        }
    }
    let frac = bad as f64 / (W * H) as f64;
    assert!(
        frac <= 0.001,
        "{name}: {:.3}% of pixels exceed ΔE 2.0",
        frac * 100.0
    );
}

#[test]
fn cpu_fallback_is_solid_fill() {
    // Before any source (and whenever there is no GPU) the surface is a solid
    // fallback fill of the right size.
    let w = ShaderWidget::new(8, 8, Color::srgb8(10, 20, 30, 255));
    assert_eq!((w.image().width(), w.image().height()), (8, 8));
    for px in w.image().pixels().chunks_exact(4) {
        assert_eq!([px[0], px[1], px[2]], [10, 20, 30]);
    }
}

#[test]
#[ignore = "needs a GPU/software adapter; run with --ignored"]
fn three_sample_shaders_match_goldens() {
    let mut w = ShaderWidget::new(W, H, Color::BLACK);
    if !w.has_gpu() {
        eprintln!("no GPU adapter; skipping shader goldens");
        return;
    }
    w.set_params([0.2, 0.7, 0.9, 1.0]);
    assert!(w.set_source(GRADIENT).is_none());
    check_golden("shader_gradient", w.image());
    assert!(w.set_source(SOLID).is_none());
    check_golden("shader_solid", w.image());
    assert!(w.set_source(RINGS).is_none());
    check_golden("shader_rings", w.image());
}

#[test]
#[ignore = "needs a GPU/software adapter; run with --ignored"]
fn broken_shader_keeps_last_frame_and_reports_e0201() {
    let mut w = ShaderWidget::new(W, H, Color::BLACK);
    if !w.has_gpu() {
        eprintln!("no GPU adapter; skipping broken-shader test");
        return;
    }
    assert!(w.set_source(GRADIENT).is_none());
    let good = w.image().pixels().to_vec();

    // A broken edit returns E0201 and the previous frame is retained.
    let diag = w.set_source(BROKEN).expect("expected a diagnostic");
    assert_eq!(diag.code, "E0201");
    assert_eq!(w.image().pixels(), good.as_slice());
    assert_eq!(w.diagnostic().unwrap().code, "E0201");
}
