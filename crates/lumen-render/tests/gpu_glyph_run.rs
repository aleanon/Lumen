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
        glyphs: vec![PlacedGlyph {
            image: 0,
            x,
            y,
            w: 10.0,
            h: 10.0,
        }],
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

/// R6.5: glyphs that spill past page 0 must sample **their own** page.
///
/// The atlas packer always supported pages; the GPU pinned `max_pages` to 1
/// because the texture was a single 2-D layer and the instance stream had no
/// page index — so a second page would have sampled page 0's texels. The scope
/// doc called this out as the reason R6.5 was "not a one-line change".
///
/// Nothing in the existing suite reaches page 1: every scene is a handful of
/// glyphs in a 1024×1024 page. So this fills page 0 with large distinct bitmaps
/// and then draws a probe that **cannot fit page 0's leftovers**, which is what
/// forces it onto page 1.
///
/// That detail is load-bearing. The first version of this test used a 10×10
/// probe and passed even with the page index hard-coded to 0 — a 5×5 grid of
/// 200px cells leaves 24px margins, and a small glyph packs happily into them.
/// A multi-page test whose probe never leaves page 0 asserts nothing.
#[test]
fn glyphs_on_a_second_atlas_page_sample_their_own_page() {
    let Some(gpu) = Wgpu::new() else {
        eprintln!("gpu_glyph_run: no wgpu adapter; skipping");
        return;
    };

    // 1024×1024 pages, 200×200 cells: 5×5 = 25 fills page 0, and nothing that
    // size fits the 24px margins left over. Distinct keys, or the packer dedups
    // them into a single slot.
    const CELL: u32 = 200;
    const FILLERS: u32 = 25;

    let mut dl = DisplayList::new();
    for k in 0..FILLERS {
        dl.glyph_images.push(GlyphImage {
            key: 1000 + k as u64,
            width: CELL,
            height: CELL,
            coverage: vec![255u8; (CELL * CELL) as usize],
        });
    }
    // The probe: left half covered, right half empty. Every filler is SOLID, so
    // sampling the wrong page tints the right half too — that asymmetry is the
    // discriminator, not the colour.
    let mut cov = vec![0u8; (CELL * CELL) as usize];
    for y in 0..CELL as usize {
        for x in 0..(CELL / 2) as usize {
            cov[y * CELL as usize + x] = 255;
        }
    }
    dl.glyph_images.push(GlyphImage {
        key: 7777,
        width: CELL,
        height: CELL,
        coverage: cov,
    });
    let probe = dl.glyph_images.len() as u32 - 1;

    // Fillers off-screen (they only need atlas space); the probe over the frame.
    let mut glyphs: Vec<PlacedGlyph> = (0..FILLERS)
        .map(|k| PlacedGlyph {
            image: k,
            x: -1000.0,
            y: -1000.0,
            w: 1.0,
            h: 1.0,
        })
        .collect();
    glyphs.push(PlacedGlyph {
        image: probe,
        x: 0.0,
        y: 0.0,
        w: W as f32,
        h: H as f32,
    });
    let id = dl.add_run(GlyphRun { glyphs });
    dl.push(DrawCmd::GlyphRun {
        run: id,
        brush: Brush::Solid(Color::srgb8(0xd0, 0x10, 0x10, 0xff)),
        rect: Rect::new(0.0, 0.0, W as f64, H as f64),
    });

    let img = gpu.render(&dl, W, H, Color::WHITE);

    // Well left of the 50% boundary: covered, so tinted.
    let left = px(&img, 8, 16);
    assert!(
        left[0] > 150 && left[1] < 90,
        "left half of the probe should be tinted, got {left:?} — white here \
         means the glyph did not render at all"
    );
    // Well right of it: zero coverage, so untouched background. This is the
    // assertion that fails when the page index is wrong.
    let right = px(&img, 56, 16);
    assert!(
        right.iter().take(3).all(|&c| c > 240),
        "right half of the probe has zero coverage and must stay white; got \
         {right:?}, which means the glyph sampled a different atlas page"
    );
}
