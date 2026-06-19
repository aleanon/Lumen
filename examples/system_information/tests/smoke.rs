use lumen_core::geometry::Size;

#[test]
fn shows_host_facts() {
    let mut a = system_information::main_app().run_headless(Size::new(520.0, 460.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("System information"));
    assert!(t.contains("OPERATING SYSTEM") && t.contains("ARCHITECTURE"));
    assert!(t.contains("LOGICAL CPUS") && t.contains("ONLINE"));
    // the accent status dot / CPU value paints green somewhere.
    let img = a.screenshot();
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[1] > 150 && p[0] < 130 && p[2] < 160),
        "accent green painted"
    );
}
