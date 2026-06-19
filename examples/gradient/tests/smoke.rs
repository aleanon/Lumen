use lumen_core::geometry::Size;

#[test]
fn shows_gradient_hero_and_chips() {
    let mut a = gradient::main_app().run_headless(Size::new(520.0, 480.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("Gradients") && t.contains("Ocean") && t.contains("Lime"));
    let img = a.screenshot();
    // hero ramp goes blue (left) to red (right): both ends present somewhere.
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[2] > 180 && p[0] < 120),
        "blue stop painted"
    );
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[0] > 180 && p[2] < 120),
        "red stop painted"
    );
}
