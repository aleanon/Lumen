//! The `complex-scripts` feature is a **3.62 MB** decision — 69% of Lumen's
//! binary-size gap against iced — so what it does and does not buy is pinned
//! here rather than left to a manifest comment. The comment it replaces was
//! wrong on both counts: it claimed parley *panics* without the feature, and
//! that the feature is needed *for CJK*.
//!
//! See `docs/binary-size-2026-08-22.md` and `examples/cjk_probe.rs`.

use lumen_text::{TextEngine, TextStyle};

const WRAP: f32 = 160.0;
const JA: &str = "日本語のテキストは、単語の区切りが空白ではありません。";
const TH: &str = "ภาษาไทยไม่มีการเว้นวรรคระหว่างคำ";
const LATIN: &str = "The quick brown fox jumps over the lazy dog.";

fn width(s: &str) -> f32 {
    let mut e = TextEngine::new();
    let ts = TextStyle {
        font_size: 16.0,
        ..Default::default()
    };
    e.shaped(s, &ts, Some(WRAP), Default::default()).width()
}

/// True either way: the feature must never be load-bearing for *not crashing*.
/// The manifest used to claim parley panics with "no segmentation model for
/// language: ja" without it. It does not — ICU records a data error and the
/// segmenter falls back.
#[test]
fn text_in_any_script_shapes_without_panicking() {
    for s in [JA, TH, LATIN] {
        assert!(width(s) > 0.0, "shaping produced nothing for {s:?}");
    }
}

/// Latin never depended on the dictionary, so it must be identical in both
/// builds — this is the control that stops the two tests below from passing
/// for some unrelated reason.
#[test]
fn latin_wraps_within_the_limit_either_way() {
    assert!(
        width(LATIN) <= WRAP + 0.5,
        "latin overflowed a {WRAP} px wrap at {} px",
        width(LATIN)
    );
}

/// **CJK does not need the dictionary to wrap.** Japanese has line-break
/// opportunities between most characters, so it honours the wrap width with or
/// without `complex-scripts`. This is the claim that made the feature look
/// mandatory; it is false, and this test fails if anyone reinstates it.
#[test]
fn cjk_wraps_with_or_without_the_dictionary() {
    let w = width(JA);
    assert!(
        w <= WRAP + 0.5,
        "Japanese overflowed a {WRAP} px wrap at {w} px — CJK wrapping is \
         supposed to be dictionary-independent"
    );
}

/// **Thai is what the dictionary actually buys.** With it, Thai wraps; without
/// it there are no break opportunities and the line overflows. Lao, Khmer and
/// Burmese share the mechanism (they are the other dictionaries in `cjdict`).
///
/// Asserted in both directions so the feature cannot be silently dropped from
/// the default set, or silently made unconditional again.
#[test]
fn thai_wrapping_is_exactly_what_the_feature_controls() {
    let w = width(TH);
    if cfg!(feature = "complex-scripts") {
        assert!(
            w <= WRAP + 0.5,
            "Thai overflowed a {WRAP} px wrap at {w} px WITH complex-scripts — \
             the dictionary is not reaching the segmenter"
        );
    } else {
        assert!(
            w > WRAP + 0.5,
            "Thai wrapped at {w} px WITHOUT complex-scripts — if this now works, \
             the 3.62 MB dictionary may no longer be needed at all"
        );
    }
}
