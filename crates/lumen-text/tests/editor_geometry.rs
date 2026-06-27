//! Caret/selection geometry on a laid-out block (drives widget caret rendering
//! + click-to-place). Deterministic via the single bundled font.

use lumen_core::Color;
use lumen_text::{TextAlign, TextEngine, TextStyle};

fn style() -> TextStyle {
    TextStyle {
        font_size: 20.0,
        color: Color::BLACK,
        weight: 400.0,
        line_height: None,
        letter_spacing: 0.0,
    }
}

#[test]
fn caret_advances_left_to_right_single_line() {
    let mut eng = TextEngine::new();
    let block = eng.layout("hello", style(), &[], None, TextAlign::Start);
    let (x0, y0, h) = block.caret_pos(0);
    let (x_mid, _, _) = block.caret_pos(3);
    let (x_end, _, _) = block.caret_pos(5);
    assert!(
        x0 <= 0.5,
        "caret at offset 0 sits at the left edge (got {x0})"
    );
    assert!(x_mid > x0, "caret moves right as the offset grows");
    assert!(x_end > x_mid, "caret at the end is rightmost");
    assert!(h > 0.0, "caret has the line height");
    assert!(y0 >= 0.0);
    // End caret roughly matches the measured text width.
    assert!(
        (x_end - block.width()).abs() < 2.0,
        "end caret {x_end} ≈ width {}",
        block.width()
    );
}

#[test]
fn hit_test_is_the_inverse_of_caret_pos() {
    let mut eng = TextEngine::new();
    let block = eng.layout("hello world", style(), &[], None, TextAlign::Start);
    for byte in [0usize, 2, 5, 8, 11] {
        let (x, y, h) = block.caret_pos(byte);
        // Probe just right of the caret, mid-line.
        let got = block.hit_to_byte(x + 0.5, y + h * 0.5);
        assert_eq!(got, byte, "hit-test at caret({byte}) → {got}");
    }
}

#[test]
fn selection_spans_multiple_lines_when_wrapped() {
    let mut eng = TextEngine::new();
    // Narrow wrap width forces several visual lines.
    let block = eng.layout(
        "the quick brown fox jumps over the lazy dog",
        style(),
        &[],
        Some(80.0),
        TextAlign::Start,
    );
    let full = "the quick brown fox jumps over the lazy dog".len();
    let rects = block.selection_rects(0, full);
    assert!(
        rects.len() >= 2,
        "wrapped full-selection spans ≥2 lines, got {}",
        rects.len()
    );
    // Each rect is non-empty.
    for (x0, y0, x1, y1) in &rects {
        assert!(x1 > x0 && y1 > y0, "non-empty selection rect");
    }

    // A collapsed range yields no (or empty) highlight.
    let none = block.selection_rects(3, 3);
    assert!(none.iter().all(|(x0, _, x1, _)| (x1 - x0).abs() < 0.01));
}
