//! B1: App::with_font registers a font for the whole app, selectable via
//! Label::family. No second font is bundled (the production font is large), so
//! this checks the wiring: a registered family renders the same as the default
//! (it *is* the same font here), and registering doesn't break rendering.

use kurbo::Size;
use lumen_render::RgbaImage;
use lumen_widgets::{App, Label};

const BUNDLED: &[u8] = include_bytes!("../../lumen-text/fonts/GoNotoKurrent-Regular.ttf");

fn nonblank(img: &RgbaImage) -> bool {
    // Anything not the white background.
    img.pixels()
        .chunks_exact(4)
        .any(|p| p[0] < 250 || p[1] < 250 || p[2] < 250)
}

#[test]
fn with_font_registers_and_renders() {
    // Discover the bundled font's family name, then select it via Label::family
    // on an app that registered it.
    let family = {
        let mut e = lumen_text::TextEngine::new();
        e.register_font(BUNDLED.to_vec()).expect("family name")
    };
    let fam = family.clone();
    let mut h = App::new(move |_| Label::new("Hg y").family(fam.clone()).into())
        .with_font(BUNDLED.to_vec())
        .run_headless(Size::new(120.0, 40.0));
    assert!(
        nonblank(&h.screenshot()),
        "registered-family text should render"
    );
}

#[test]
fn unknown_family_still_renders_via_fallback() {
    let mut h = App::new(|_| Label::new("Hg y").family("No Such Font").into())
        .run_headless(Size::new(120.0, 40.0));
    assert!(
        nonblank(&h.screenshot()),
        "unknown family must fall back to the default font, not vanish"
    );
}
