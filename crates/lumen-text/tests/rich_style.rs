//! B2: line-height and letter-spacing change layout (and default to no-op, so
//! the goldens are unaffected — verified by the golden suite staying green).

use lumen_text::{TextEngine, TextStyle};

#[test]
fn letter_spacing_widens_a_line() {
    let mut e = TextEngine::new();
    let base = TextStyle::default();
    let w0 = e
        .layout("AAAA", base, &[], None, lumen_text::TextAlign::Start)
        .width();
    let w1 = e
        .layout(
            "AAAA",
            base.letter_spacing(4.0),
            &[],
            None,
            lumen_text::TextAlign::Start,
        )
        .width();
    assert!(w1 > w0 + 8.0, "tracking widened the line: {w0} -> {w1}");
}

#[test]
fn line_height_grows_wrapped_block() {
    let mut e = TextEngine::new();
    let base = TextStyle::default();
    let max = Some(48.0); // force wrapping to several lines
    let h0 = e
        .layout(
            "aaa bbb ccc ddd eee",
            base,
            &[],
            max,
            lumen_text::TextAlign::Start,
        )
        .height();
    let h1 = e
        .layout(
            "aaa bbb ccc ddd eee",
            base.line_height(2.0),
            &[],
            max,
            lumen_text::TextAlign::Start,
        )
        .height();
    assert!(
        h1 > h0,
        "line-height increased wrapped height: {h0} -> {h1}"
    );
}
