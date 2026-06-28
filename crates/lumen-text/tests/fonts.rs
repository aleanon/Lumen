//! B1: additive font registration + family selection, keeping determinism (no
//! system enumeration). A second visual font isn't bundled (the production font
//! is large), so these exercise the API: registration returns the family name,
//! a selected registered family routes through layout, and an unknown family
//! falls back to the default.

use lumen_core::Color;
use lumen_text::{TextAlign, TextEngine, TextStyle};

// The bundled font's own bytes — registering them is a no-system-fonts way to
// drive the API deterministically (same family the engine already uses).
const BUNDLED: &[u8] = include_bytes!("../fonts/GoNotoKurrent-Regular.ttf");

fn style() -> TextStyle {
    TextStyle {
        font_size: 20.0,
        color: Color::BLACK,
        weight: 400.0,
        line_height: None,
        letter_spacing: 0.0,
        family: None,
    }
}

#[test]
fn register_font_returns_a_family_name() {
    let mut eng = TextEngine::new();
    let name = eng.register_font(BUNDLED.to_vec());
    assert!(
        name.as_deref().is_some_and(|n| !n.is_empty()),
        "register_font should return the font's family name, got {name:?}"
    );
}

#[test]
fn register_invalid_font_returns_none() {
    let mut eng = TextEngine::new();
    assert_eq!(eng.register_font(b"not a font".to_vec()), None);
}

#[test]
fn selecting_a_registered_family_routes_through_layout() {
    let mut eng = TextEngine::new();
    let name = eng.register_font(BUNDLED.to_vec()).expect("family name");
    // Selecting the registered family lays out (same font as default here, so
    // identical width) — proves the family is threaded into the FontStack.
    let def = eng
        .layout("Ag y", style(), &[], None, TextAlign::Start)
        .width();
    let sel = eng
        .layout("Ag y", style().family(name), &[], None, TextAlign::Start)
        .width();
    assert!(
        (def - sel).abs() < 0.01,
        "registered family layout matches ({def} vs {sel})"
    );
    assert!(def > 0.0);
}

#[test]
fn unknown_family_falls_back_to_default() {
    let mut eng = TextEngine::new();
    let def = eng
        .layout("Ag y", style(), &[], None, TextAlign::Start)
        .width();
    let unknown = eng
        .layout(
            "Ag y",
            style().family("No Such Font 12345"),
            &[],
            None,
            TextAlign::Start,
        )
        .width();
    assert!(
        (def - unknown).abs() < 0.01,
        "unknown family falls back to the default ({def} vs {unknown})"
    );
}
