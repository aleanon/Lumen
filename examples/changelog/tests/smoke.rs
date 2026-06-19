use lumen_core::geometry::Size;

#[test]
fn shows_versions_and_badges() {
    let mut a = changelog::main_app().run_headless(Size::new(560.0, 560.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("Changelog") && t.contains("1.2.0") && t.contains("1.1.0"));
    assert!(
        t.contains("ADDED")
            && t.contains("FIXED")
            && t.contains("CHANGED")
            && t.contains("REMOVED")
    );
    let img = a.screenshot();
    // an "added" badge paints green text on a dark tinted pill.
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[1] > 120 && p[0] < 120 && p[2] < 130),
        "green added badge painted"
    );
}
