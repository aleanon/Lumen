//! Font weight: heavier text renders more ink (synthesized bold from the single
//! bundled weight).
use lumen_core::Color;
use lumen_text::{TextAlign, TextEngine, TextStyle};

fn ink(weight: f32) -> u64 {
    let mut eng = TextEngine::new();
    let style = TextStyle {
        font_size: 32.0,
        color: Color::BLACK,
        weight,
        line_height: None,
        letter_spacing: 0.0,
        family: None,
        features: None,
        variations: None,
        italic: false,
        align: TextAlign::Start,
    };
    let block = eng.layout("Bold", style, &[], None, TextAlign::Start);
    let img = block.render(0, 0, Color::WHITE);
    // Sum of "darkness" (255 - luminance proxy) over all pixels.
    img.pixels()
        .chunks_exact(4)
        .map(|p| 255 - p[0] as u64)
        .sum()
}

#[test]
fn heavier_weight_renders_more_ink() {
    let regular = ink(400.0);
    let bold = ink(700.0);
    assert!(regular > 0, "regular text drew something");
    assert!(
        bold > regular,
        "bold ({bold}) should lay down more ink than regular ({regular})"
    );
}

/// PROP1 cache correctness: `italic` must be part of the shape-cache key.
///
/// This uses ONE engine deliberately. An earlier version of the `font-style`
/// test built a fresh `App` per case, so every shape was a cache miss and the
/// missing key field was invisible — the bug only appears when two differently
/// styled runs of the SAME string share an engine, which is the normal case in
/// a real app.
#[test]
fn italic_is_part_of_the_shape_cache_key() {
    let mut te = lumen_text::TextEngine::new();
    let upright = TextStyle {
        font_size: 32.0,
        ..Default::default()
    };
    let italic = TextStyle {
        italic: true,
        ..upright.clone()
    };
    let a =
        te.shaped("Hll", &upright, None, TextAlign::Start)
            .render(64, 48, lumen_core::Color::WHITE);
    let b =
        te.shaped("Hll", &italic, None, TextAlign::Start)
            .render(64, 48, lumen_core::Color::WHITE);
    assert_ne!(
        a.pixels(),
        b.pixels(),
        "italic and upright must not share a shape-cache entry"
    );
}

/// PROP1: OpenType feature settings must reach the shaper AND key the cache.
///
/// `smcp` (small caps) is in the bundled face, so enabling it substitutes
/// different glyphs — a visible, pixel-level difference. One engine for both,
/// for the same reason as the italic test: a fresh engine per case would make a
/// missing `ShapeKey` field invisible.
#[test]
fn font_features_reach_the_shaper_and_key_the_cache() {
    let mut te = lumen_text::TextEngine::new();
    let plain = TextStyle {
        font_size: 32.0,
        ..Default::default()
    };
    let smallcaps = TextStyle {
        features: Some("\"smcp\" 1".into()),
        ..plain.clone()
    };
    let a = te.shaped("hello", &plain, None, TextAlign::Start).render(
        160,
        48,
        lumen_core::Color::WHITE,
    );
    let b = te
        .shaped("hello", &smallcaps, None, TextAlign::Start)
        .render(160, 48, lumen_core::Color::WHITE);
    assert_ne!(
        a.pixels(),
        b.pixels(),
        "`smcp` must substitute small-cap glyphs, and must not share a cache entry"
    );
}
