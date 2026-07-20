use lumen_core::geometry::Size;

const CANNED: &str = r#"{"name":"pikachu","height":4,"types":[]}"#;

#[test]
fn renders_from_an_injected_transport() {
    // Offline by construction: the transport is canned JSON.
    let mut a =
        pokedex::app_with(|_url| Ok(CANNED.to_string())).run_headless(Size::new(420.0, 320.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("pikachu — height 4"), "decoded + rendered: {t}");
}

#[test]
fn transport_errors_surface_as_data() {
    let mut a =
        pokedex::app_with(|_| Err("dns failure".into())).run_headless(Size::new(420.0, 320.0));
    a.pump();
    assert!(a
        .semantics_json()
        .to_string()
        .contains("error: dns failure"));
}
