//! R3.3: the GPU `DrawCmd::GlyphRun` path packs coverage bitmaps into the atlas
//! and draws instanced quads, tinted by the run color, at the right place.
//! Self-skips without a GPU adapter.
#![cfg(feature = "wgpu")]

use kurbo::Rect;
use lumen_core::Color;
use lumen_render::gpu::Wgpu;
use lumen_render::{Brush, DisplayList, DrawCmd, GlyphImage, GlyphRun, PlacedGlyph};

const W: u32 = 64;
const H: u32 = 32;

fn px(img: &lumen_render::RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * img.width() + x) * 4) as usize;
    let p = img.pixels();
    [p[i], p[i + 1], p[i + 2], p[i + 3]]
}

/// A display list with one glyph: a 10×10 full-coverage square at (x, y) in the
/// given color, on a white background.
fn glyph_list(color: Color, x: f32, y: f32) -> DisplayList {
    let mut dl = DisplayList::new();
    dl.glyph_images.push(GlyphImage {
        key: 1,
        width: 10,
        height: 10,
        coverage: vec![255u8; 100],
    });
    let run = GlyphRun {
        glyphs: vec![PlacedGlyph { image: 0, x, y }],
    };
    let id = dl.add_run(run);
    dl.push(DrawCmd::GlyphRun {
        run: id,
        brush: Brush::Solid(color),
        rect: Rect::new(x as f64, y as f64, x as f64 + 10.0, y as f64 + 10.0),
    });
    dl
}

#[test]
fn gpu_glyph_run_tints_coverage_at_position() {
    let Some(gpu) = Wgpu::new() else {
        eprintln!("gpu_glyph_run: no wgpu adapter; skipping");
        return;
    };
    let dl = glyph_list(Color::srgb8(0xd0, 0x10, 0x10, 0xff), 20.0, 11.0);
    let img = gpu.render(&dl, W, H, Color::WHITE);

    // Inside the square (20..30, 11..21): the red tint, fully opaque coverage.
    let inside = px(&img, 24, 15);
    assert!(
        inside[0] > 150 && inside[1] < 90 && inside[2] < 90,
        "glyph interior should be the red tint, got {inside:?}"
    );
    // Outside the square: untouched white background.
    let outside = px(&img, 2, 2);
    assert!(
        outside.iter().take(3).all(|&c| c > 240),
        "outside the glyph should stay white, got {outside:?}"
    );
}

#[test]
fn gpu_glyph_run_matches_cpu_for_opaque_coverage() {
    let Some(gpu) = Wgpu::new() else {
        eprintln!("gpu_glyph_run: no wgpu adapter; skipping");
        return;
    };
    // Full-coverage opaque black has no AA, so GPU (linear) and CPU (gamma)
    // composite identically — a clean cross-backend parity check of placement.
    let dl = glyph_list(Color::BLACK, 18.0, 9.0);
    let g = gpu.render(&dl, W, H, Color::WHITE);
    let c = lumen_render::cpu::render(&dl, W, H, Color::WHITE);
    let mut differ = 0;
    for y in 0..H {
        for x in 0..W {
            let (a, b) = (px(&g, x, y), px(&c, x, y));
            if (0..3).any(|k| (a[k] as i32 - b[k] as i32).abs() > 4) {
                differ += 1;
            }
        }
    }
    let frac = differ as f64 / (W * H) as f64;
    eprintln!("gpu vs cpu opaque glyph: {:.3}% differ", frac * 100.0);
    assert!(frac < 0.01, "opaque glyph should match CPU (got {frac})");
}
