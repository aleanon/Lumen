//! T1.5 acceptance: goldens for selection rendering. (IME pre-edit/commit
//! sequences incl. CJK are covered by the `editor::` unit tests.)

use lumen_core::Color;
use lumen_render::RgbaImage;
use lumen_text::{TextEditor, TextEngine, TextStyle};
use std::path::PathBuf;

fn check_golden(name: &str, img: &RgbaImage) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/cpu")
        .join(format!("{name}.png"));
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
            "golden mismatch for {name}: {} px differ",
            img.diff_count(&expected)
        );
    }
}

fn style() -> TextStyle {
    TextStyle {
        font_size: 24.0,
        color: Color::BLACK,
    }
}

#[test]
fn selection_highlight_golden() {
    let mut eng = TextEngine::new();
    let mut ed = TextEditor::new("Hello world");
    ed.set_selection(0, 5); // select "Hello"
    let text = ed.display_text();
    let (a, b) = ed.selection();
    let x0 = eng.measure_prefix(&text, style(), a);
    let x1 = eng.measure_prefix(&text, style(), b);
    let block = eng.layout(&text, style(), &[], None, lumen_text::TextAlign::Start);
    let img = block.render_with_selection(
        0,
        0,
        Color::srgb8(255, 255, 255, 255),
        x0,
        x1,
        Color::srgb8(0xb3, 0xd7, 0xff, 0xff),
    );
    check_golden("selection_hello", &img);
}

#[test]
fn cjk_selection_golden() {
    // selection across CJK glyphs (byte offsets are multi-byte)
    let mut eng = TextEngine::new();
    let mut ed = TextEditor::new("你好世界");
    ed.set_selection(0, 6); // first two chars "你好" (3 bytes each)
    let text = ed.display_text();
    let (a, b) = ed.selection();
    let x0 = eng.measure_prefix(&text, style(), a);
    let x1 = eng.measure_prefix(&text, style(), b);
    let block = eng.layout(&text, style(), &[], None, lumen_text::TextAlign::Start);
    let img = block.render_with_selection(
        0,
        0,
        Color::srgb8(255, 255, 255, 255),
        x0,
        x1,
        Color::srgb8(0xb3, 0xd7, 0xff, 0xff),
    );
    check_golden("selection_cjk", &img);
}
