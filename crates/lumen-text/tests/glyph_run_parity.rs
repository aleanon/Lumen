//! R3.2a: the new `DrawCmd::GlyphRun` paint path must match the old
//! pre-rasterized sprite path within tolerance, through the same CPU renderer.

use kurbo::Rect;
use lumen_core::Color;
use lumen_render::{cpu, Brush, DisplayList, DrawCmd, Filter, ImageId};
use lumen_text::{TextAlign, TextEngine, TextStyle};

fn style(size: f32) -> TextStyle {
    TextStyle {
        font_size: size,
        color: Color::BLACK,
        weight: 400.0,
        line_height: None,
        letter_spacing: 0.0,
    }
}

/// Fraction of pixels whose any channel differs by more than `thresh`.
fn diff_fraction(a: &lumen_render::RgbaImage, b: &lumen_render::RgbaImage, thresh: u8) -> f64 {
    assert_eq!(
        (a.width(), a.height()),
        (b.width(), b.height()),
        "same size"
    );
    let (pa, pb) = (a.pixels(), b.pixels());
    let mut differ = 0usize;
    for i in (0..pa.len()).step_by(4) {
        let d = (0..4)
            .map(|c| (pa[i + c] as i32 - pb[i + c] as i32).unsigned_abs())
            .max()
            .unwrap_or(0);
        if d > thresh as u32 {
            differ += 1;
        }
    }
    differ as f64 / (pa.len() / 4) as f64
}

#[test]
fn glyph_run_matches_sprite_within_tolerance() {
    let mut eng = TextEngine::new();
    let text = "gypq Hello, jaWQ!";
    for size in [14.0_f32, 22.0, 40.0] {
        let block = eng.layout(text, style(size), &[], None, TextAlign::Start);
        let w = block.width().ceil().max(1.0) as u32;
        let h = block.height().ceil().max(1.0) as u32;

        // Path A — the old sprite: rasterize the whole string to a transparent
        // image and blit it as a DrawCmd::Image (what the widget did pre-R3).
        let sprite = block.render(0, 0, Color::srgb8(255, 255, 255, 0));
        let mut dl_a = DisplayList::new();
        dl_a.images.push(sprite);
        dl_a.push(DrawCmd::Image {
            id: ImageId(0),
            src_rect: Rect::new(0.0, 0.0, w as f64, h as f64),
            dst_rect: Rect::new(0.0, 0.0, w as f64, h as f64),
            quality: Filter::Nearest,
        });
        let img_a = cpu::render(&dl_a, w, h, Color::WHITE);

        // Path B — the new GlyphRun: positioned glyphs + coverage bitmaps.
        let (run, images) = block.glyph_run(0.0, 0.0);
        let mut dl_b = DisplayList::new();
        dl_b.glyph_images = images;
        let id = dl_b.add_run(run);
        dl_b.push(DrawCmd::GlyphRun {
            run: id,
            brush: Brush::Solid(Color::BLACK),
            rect: Rect::new(0.0, 0.0, w as f64, h as f64),
        });
        let img_b = cpu::render(&dl_b, w, h, Color::WHITE);

        let frac = diff_fraction(&img_a, &img_b, 12);
        eprintln!("size {size}: {:.4}% pixels differ >12", frac * 100.0);
        assert!(
            frac < 0.02,
            "GlyphRun vs sprite at size {size}: {:.2}% pixels differ (>2%)",
            frac * 100.0
        );
    }
}

#[test]
fn glyph_run_dedups_repeated_glyphs() {
    let mut eng = TextEngine::new();
    // "aaaa" — one unique glyph + a space-free run; the image table holds 1 entry.
    let block = eng.layout("aaaa", style(20.0), &[], None, TextAlign::Start);
    let (run, images) = block.glyph_run(0.0, 0.0);
    assert_eq!(run.glyphs.len(), 4, "four placed glyphs");
    assert_eq!(images.len(), 1, "deduped to one coverage bitmap");
    assert!(run.glyphs.iter().all(|g| g.image == 0));
}
