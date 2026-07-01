//! Item 5: TextBlock::metrics() names the line-height class of clipping —
//! content_height (glyph extent) vs box_height (reserved height).

use lumen_core::Color;
use lumen_text::{TextAlign, TextEngine, TextStyle};

fn style(line_height: Option<f32>) -> TextStyle {
    TextStyle {
        font_size: 24.0,
        color: Color::BLACK,
        weight: 400.0,
        line_height,
        letter_spacing: 0.0,
        family: None,
    }
}

#[test]
fn metrics_report_ascent_descent_and_line_count() {
    let mut eng = TextEngine::new();
    let m = eng
        .layout("gypq Ág", style(None), &[], None, TextAlign::Start)
        .metrics();
    assert_eq!(m.line_count, 1);
    assert!(m.ascent > 0.0 && m.descent > 0.0);
    assert!(m.box_height > 0.0 && m.content_height > 0.0);
}

#[test]
fn tighter_line_height_reserves_a_smaller_box() {
    let mut eng = TextEngine::new();
    let def = eng
        .layout("gypq Ág", style(None), &[], None, TextAlign::Start)
        .metrics();
    let tight = eng
        .layout("gypq Ág", style(Some(1.0)), &[], None, TextAlign::Start)
        .metrics();
    assert!(
        tight.line_height < def.line_height && tight.box_height < def.box_height,
        "tighter line-height → smaller box ({tight:?} vs {def:?})"
    );
    // A 1.0 line box is tighter than the font's declared extent (the line-height
    // hint behind a W0104; the ink check is authoritative).
    assert!(
        tight.content_height > tight.box_height + 0.5,
        "1.0 line box is tighter than the font extent ({tight:?})"
    );
}
