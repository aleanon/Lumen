use lumen_core::geometry::Size;

#[test]
fn renders_document_blocks() {
    let mut a = markdown::main_app().run_headless(Size::new(560.0, 560.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("Lumen") && t.contains("Getting started"));
    assert!(t.contains("Why a CPU renderer?"));
    // the dark code block paints a near-black panel somewhere.
    let img = a.screenshot();
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[0] < 45 && p[1] < 50 && p[2] < 60),
        "dark code panel painted"
    );
}

#[test]
fn long_paragraph_wraps_within_card() {
    // The first paragraph is long; with an explicit content width it must wrap
    // rather than overflow. We assert the laid-out text height spans >1 line.
    let mut a = markdown::main_app().run_headless(Size::new(560.0, 560.0));
    a.pump();
    fn find<'n>(
        n: &'n lumen_core::semantics::SemanticsNode,
        needle: &str,
    ) -> Option<&'n lumen_core::semantics::SemanticsNode> {
        if n.label.contains(needle) {
            return Some(n);
        }
        n.children.iter().find_map(|c| find(c, needle))
    }
    let doc = a.semantics_doc();
    let para = find(&doc.root, "AI-first GUI framework").expect("paragraph present");
    let h = para.bounds.y1 - para.bounds.y0;
    assert!(h > 24.0, "paragraph wrapped to multiple lines (height {h})");
}
