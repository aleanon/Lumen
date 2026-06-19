use lumen_core::geometry::Size;

#[test]
fn rasterizes_icon_gallery() {
    let mut a = svg::main_app().run_headless(Size::new(520.0, 420.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("SVG icons"));
    assert!(
        t.contains("Check") && t.contains("Heart") && t.contains("Gem") && t.contains("Target")
    );
    let img = a.screenshot();
    // the green check badge and the red heart both rasterize.
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[1] > 130 && p[0] < 110 && p[2] < 110),
        "green badge painted"
    );
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[0] > 180 && p[1] < 110 && p[2] < 110),
        "red heart painted"
    );
}
