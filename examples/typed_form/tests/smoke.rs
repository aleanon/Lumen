use lumen_core::geometry::Size;

#[test]
fn shows_typed_form_fields() {
    let mut a = typed_form::main_app().run_headless(Size::new(460.0, 560.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("Preferences"));
    assert!(t.contains("DISPLAY NAME") && t.contains("Ada Lovelace"));
    assert!(t.contains("Email me product updates"));
    assert!(t.contains("Cancel") && t.contains("Save"));
}
