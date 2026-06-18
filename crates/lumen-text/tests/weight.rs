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
