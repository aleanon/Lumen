use lumen_core::geometry::Size;

#[test]
fn shows_three_named_spinners() {
    let mut a = loading_spinners::main_app().run_headless(Size::new(520.0, 360.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(
        t.contains("Loading") && t.contains("Cyan") && t.contains("Violet") && t.contains("Rose")
    );
    let img = a.screenshot();
    // cyan arc present (high blue + green, low red).
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[2] > 180 && p[1] > 120 && p[0] < 120),
        "cyan arc painted"
    );
}
