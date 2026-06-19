use lumen_core::geometry::Size;

#[test]
fn shows_four_severities() {
    let mut a = toast::main_app().run_headless(Size::new(520.0, 460.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("Heads up") && t.contains("Saved"));
    assert!(t.contains("Low disk space") && t.contains("Upload failed"));
    let img = a.screenshot();
    // success accent bar is green; danger accent bar is red.
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[1] > 130 && p[0] < 120 && p[2] < 130),
        "success bar green"
    );
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[0] > 180 && p[1] < 110 && p[2] < 110),
        "danger bar red"
    );
}
