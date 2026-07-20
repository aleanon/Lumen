use lumen_core::geometry::Size;

#[test]
fn encodes_and_paints_dark_modules() {
    let mut a = qr::main_app().run_headless(Size::new(420.0, 380.0));
    a.pump();
    let img = a.screenshot();
    // A QR code paints a meaningful number of dark pixels on the white card.
    let dark = img
        .pixels()
        .chunks_exact(4)
        .filter(|p| p[0] < 60 && p[1] < 60 && p[2] < 60)
        .count();
    assert!(dark > 500, "dark modules painted ({dark})");
    // Different content ⇒ different code.
    let text = a.runtime().signal("text", String::new);
    text.set(a.runtime(), "something completely different".into());
    a.pump();
    assert_ne!(a.screenshot().pixels(), img.pixels());
}
