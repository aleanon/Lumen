use lumen_core::geometry::Size;

#[test]
fn shows_explorer_and_source() {
    let mut a = pane_grid::main_app().run_headless(Size::new(600.0, 420.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("EXPLORER") && t.contains("main.rs"));
    assert!(t.contains("Editor"));
    // the split starts centred (ratio 0.50).
    assert!(t.contains("0.50"), "split ratio exposed");
}
