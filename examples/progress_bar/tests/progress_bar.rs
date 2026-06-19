use lumen_core::geometry::Size;

#[test]
fn shows_status_bars() {
    let mut a = progress_bar::main_app().run_headless(Size::new(480.0, 500.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("Storage") && t.contains("72%"));
    let img = a.screenshot();
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[0] > 180 && p[1] < 90 && p[2] < 90),
        "danger fill red"
    );
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[1] > 120 && p[0] < 120 && p[2] < 120),
        "success fill green"
    );
}
