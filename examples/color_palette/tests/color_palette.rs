use lumen_core::geometry::Size;

#[test]
fn shows_palette() {
    let mut a = color_palette::main_app().run_headless(Size::new(580.0, 480.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("Palette") && t.contains("Hue 255"));
    assert!(
        a.screenshot()
            .pixels()
            .chunks_exact(4)
            .any(|p| p[2] > 180 && p[0] > 40 && p[0] < 170),
        "a blue swatch painted"
    );
}
