use lumen_core::geometry::Size;

#[test]
fn shows_hero_and_filmstrip() {
    let mut a = image::main_app().run_headless(Size::new(560.0, 540.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("Image viewer") && t.contains("5 frames"));
    let img = a.screenshot();
    // the test pattern is fully saturated: strong reds and strong greens appear.
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[0] > 200 && p[1] < 90),
        "saturated red present"
    );
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[1] > 200 && p[0] < 90),
        "saturated green present"
    );
}
