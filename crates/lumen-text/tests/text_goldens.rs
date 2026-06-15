//! T0.6 acceptance: goldens for latin/CJK/bidi/emoji/wrap/ellipsis (exact
//! compare on the bundled font) and a stable measurement function.

use lumen_core::Color;
use lumen_render::RgbaImage;
use lumen_text::{TextAlign, TextEngine, TextStyle};
use std::path::PathBuf;

fn golden_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/cpu")
        .join(format!("{name}.png"))
}

fn check_golden(name: &str, img: &RgbaImage) {
    let path = golden_file(name);
    if std::env::var_os("LUMEN_UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, img.to_png()).unwrap();
        return;
    }
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|_| panic!("missing golden {path:?}; run with LUMEN_UPDATE_GOLDENS=1"));
    let expected = RgbaImage::from_png(&bytes).unwrap();
    if img != &expected {
        let actual = path.with_extension("actual.png");
        std::fs::write(&actual, img.to_png()).unwrap();
        panic!(
            "golden mismatch for {name}: {} px differ; wrote {actual:?}",
            img.diff_count(&expected)
        );
    }
}

fn black() -> TextStyle {
    TextStyle {
        font_size: 24.0,
        color: Color::BLACK,
    }
}
fn white() -> Color {
    Color::srgb8(255, 255, 255, 255)
}

#[test]
fn golden_latin() {
    let mut e = TextEngine::new();
    let b = e.layout("Hello, Lumen!", black(), &[], None, TextAlign::Start);
    check_golden("latin", &b.render(0, 0, white()));
}

#[test]
fn golden_cjk() {
    let mut e = TextEngine::new();
    // "Hello world" in Chinese + Japanese kana
    let b = e.layout("你好世界 こんにちは", black(), &[], None, TextAlign::Start);
    check_golden("cjk", &b.render(0, 0, white()));
}

#[test]
fn golden_bidi() {
    let mut e = TextEngine::new();
    // Latin + Arabic + Hebrew mixed (RTL runs reordered by parley)
    let b = e.layout("Hi שלום مرحبا end", black(), &[], None, TextAlign::Start);
    check_golden("bidi", &b.render(0, 0, white()));
}

#[test]
fn golden_emoji() {
    // Monochrome symbol/emoji coverage from the bundled font (color emoji is
    // out of M0 scope; see decision log).
    let mut e = TextEngine::new();
    let b = e.layout("stars ★ ☺ ♥", black(), &[], None, TextAlign::Start);
    check_golden("emoji", &b.render(0, 0, white()));
}

#[test]
fn golden_wrap() {
    let mut e = TextEngine::new();
    let b = e.layout(
        "The quick brown fox jumps over the lazy dog repeatedly.",
        black(),
        &[],
        Some(160.0),
        TextAlign::Start,
    );
    check_golden("wrap", &b.render(160, 0, white()));
}

#[test]
fn golden_ellipsis() {
    let mut e = TextEngine::new();
    let b = e.layout_ellipsized("This label is far too long to fit", black(), 140.0);
    check_golden("ellipsis", &b.render(160, 0, white()));
}

#[test]
fn golden_multistyle() {
    let mut e = TextEngine::new();
    let big_blue = TextStyle {
        font_size: 32.0,
        color: Color::srgb8(0x1a, 0x73, 0xe8, 0xff),
    };
    let b = e.layout(
        "size and color",
        black(),
        &[(9..14, big_blue)],
        None,
        TextAlign::Start,
    );
    check_golden("multistyle", &b.render(0, 0, white()));
}

#[test]
fn measurement_is_stable_across_runs() {
    let mut e = TextEngine::new();
    let w1 = e
        .layout("Stable measurement", black(), &[], None, TextAlign::Start)
        .width();
    let w2 = e
        .layout("Stable measurement", black(), &[], None, TextAlign::Start)
        .width();
    assert_eq!(w1, w2, "measurement must be deterministic");
    assert!(w1 > 0.0);

    // wrapping reduces width to <= the wrap limit
    let wrapped = e.layout(
        "The quick brown fox jumps over the lazy dog",
        black(),
        &[],
        Some(120.0),
        TextAlign::Start,
    );
    assert!(
        wrapped.width() <= 120.5,
        "wrapped width {} > limit",
        wrapped.width()
    );
    assert!(wrapped.height() > black().font_size, "multi-line height");
}
