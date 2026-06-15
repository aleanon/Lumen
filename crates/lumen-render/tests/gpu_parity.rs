//! T0.11 acceptance: GPU output matches the CPU reference renderer within the
//! perceptual threshold (05 §4): per-pixel ΔE in Oklab ≤ 2.0, and ≤ 0.1% of
//! pixels differing. `#[ignore]`d — needs a GPU/software adapter; run with
//! `cargo test -p lumen-render -- --ignored gpu_parity`.

use kurbo::Rect;
use lumen_core::Color;
use lumen_render::cpu;
use lumen_render::display_list::*;
use lumen_render::gpu::GpuRenderer;
use lumen_render::RgbaImage;

const W: u32 = 200;
const H: u32 = 150;

fn bg() -> Color {
    Color::srgb8(255, 255, 255, 255)
}

/// Opaque, integer-aligned rects + an exactly-scaled opaque image. Opaque so
/// blending (which the GPU does in linear space and the CPU in gamma space)
/// doesn't enter — that divergence is out of M0 parity scope.
fn scene() -> DisplayList {
    let mut dl = DisplayList::new();
    for (r, c) in [
        (
            Rect::new(10.0, 10.0, 110.0, 90.0),
            Color::srgb8(0x1a, 0x73, 0xe8, 0xff),
        ),
        (
            Rect::new(60.0, 50.0, 180.0, 130.0),
            Color::srgb8(0x2e, 0xa0, 0x43, 0xff),
        ),
        (
            Rect::new(130.0, 20.0, 170.0, 60.0),
            Color::srgb8(0xe8, 0x1a, 0x4b, 0xff),
        ),
    ] {
        dl.push(DrawCmd::Rect {
            rect: r,
            brush: Brush::Solid(c),
            radii: CornerRadii::ZERO,
            border: None,
        });
    }
    // 2x2 opaque checkerboard, scaled 16x to 32px at an integer origin.
    let r = Color::srgb8(220, 40, 40, 255).to_srgb8();
    let y = Color::srgb8(250, 210, 60, 255).to_srgb8();
    let mut px = Vec::new();
    for (a, b) in [(r, y), (y, r)] {
        px.extend_from_slice(&a);
        px.extend_from_slice(&b);
    }
    dl.images.push(RgbaImage::from_raw(2, 2, px));
    dl.push(DrawCmd::Image {
        id: ImageId(0),
        src_rect: Rect::new(0.0, 0.0, 2.0, 2.0),
        dst_rect: Rect::new(20.0, 100.0, 52.0, 132.0),
        quality: Filter::Nearest,
    });
    dl
}

#[test]
#[ignore = "needs a GPU/software adapter; run with --ignored gpu_parity"]
fn gpu_parity_matches_cpu() {
    let Some(gpu) = GpuRenderer::new() else {
        eprintln!("no wgpu adapter available; skipping GPU parity");
        return;
    };
    let dl = scene();
    let cpu_img = cpu::render(&dl, W, H, bg());
    let gpu_img = gpu.render(&dl, W, H, bg());

    assert_eq!(gpu_img.width(), W);
    assert_eq!(gpu_img.height(), H);

    let mut over_threshold = 0usize;
    let mut max_de = 0.0f32;
    for (a, b) in cpu_img
        .pixels()
        .chunks_exact(4)
        .zip(gpu_img.pixels().chunks_exact(4))
    {
        let ca = Color::srgb8(a[0], a[1], a[2], a[3]);
        let cb = Color::srgb8(b[0], b[1], b[2], b[3]);
        let de = ca.delta_e_oklab(cb);
        max_de = max_de.max(de);
        if de > 2.0 {
            over_threshold += 1;
        }
    }
    let total = (W * H) as usize;
    let frac = over_threshold as f64 / total as f64;
    assert!(
        frac <= 0.001,
        "GPU↔CPU parity failed: {over_threshold}/{total} px exceed ΔE 2.0 ({:.4}%), max ΔE {max_de:.3}",
        frac * 100.0
    );
}
