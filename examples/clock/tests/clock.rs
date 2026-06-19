use lumen_core::geometry::Size;

#[test]
fn renders_face_and_time() {
    let mut a = clock::main_app().run_headless(Size::new(460.0, 500.0));
    a.pump();
    assert!(a.semantics_json().to_string().contains("00:00:00"));
    assert!(
        a.screenshot()
            .pixels()
            .chunks_exact(4)
            .any(|p| p[0] > 200 && p[1] < 120 && p[2] < 150),
        "red second hand drawn"
    );
}
