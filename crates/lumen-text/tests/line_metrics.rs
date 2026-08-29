//! T1: a single line's height, without shaping the text.
//!
//! A label's height, when it is one unwrapped line, is a property of the font
//! and the size — not of the glyphs on it. Answering it without shaping is what
//! lets the lowering size a node it is not going to measure, which is the
//! prerequisite for T2 (deferred text measurement).
//!
//! The contract these tests hold is exact equality with the shaped answer. An
//! *approximation* would be worse than useless: it would move every baseline in
//! the corpus by a fraction of a pixel and be discovered as a golden diff.

use lumen_text::{TextEngine, TextStyle};

fn style(size: f32) -> TextStyle {
    TextStyle {
        font_size: size,
        ..TextStyle::default()
    }
}

/// The headline property: identical to shaping, across sizes.
#[test]
fn matches_the_shaped_height_at_every_size() {
    let mut e = TextEngine::new();
    for size in [8.0f32, 11.0, 12.5, 14.0, 16.0, 24.0, 48.0, 96.0] {
        let s = style(size);
        let shaped = e
            .shaped("x", &s, None, lumen_text::TextAlign::Start)
            .height();
        let metrics = e.line_height_for(&s);
        assert_eq!(
            metrics, shaped,
            "line_height_for disagreed with shaping at {size}px"
        );
    }
}

/// And independent of the *text*, which is the claim that makes the cache
/// sound: if two different strings in one style could have different single-line
/// heights, one cached value could not serve both.
#[test]
fn is_independent_of_the_glyphs_on_the_line() {
    let mut e = TextEngine::new();
    let s = style(16.0);
    let expected = e.line_height_for(&s);
    for text in ["x", "M", "hello world", "gypq jQ", "1234567890", "..."] {
        let shaped = e
            .shaped(text, &s, None, lumen_text::TextAlign::Start)
            .height();
        assert_eq!(
            shaped, expected,
            "single-line height varied with the text ({text:?}), so it cannot be \
             cached per style"
        );
    }
}

/// Line-height multiples and weights move it, so they must be in the key.
#[test]
fn the_key_covers_what_moves_a_baseline() {
    let mut e = TextEngine::new();
    let base = style(16.0);
    let tall = TextStyle {
        line_height: Some(2.0),
        ..style(16.0)
    };
    assert_ne!(
        e.line_height_for(&base),
        e.line_height_for(&tall),
        "a line-height multiple must not collide with the default in the cache"
    );
    let big = style(32.0);
    assert_ne!(
        e.line_height_for(&base),
        e.line_height_for(&big),
        "font size must not collide in the cache"
    );
}

/// Repeated calls are the point — the second must be the cache, not a reshape,
/// and must still agree.
#[test]
fn repeated_calls_agree() {
    let mut e = TextEngine::new();
    let s = style(14.0);
    let first = e.line_height_for(&s);
    for _ in 0..100 {
        assert_eq!(e.line_height_for(&s), first);
    }
}
