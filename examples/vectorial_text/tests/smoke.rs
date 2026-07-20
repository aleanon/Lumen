use lumen_core::geometry::Size;

#[test]
fn outlines_paint_as_vectors() {
    // The API: outlines are non-empty Béziers for real text.
    let mut eng = lumen_text::TextEngine::new();
    let block = eng.layout(
        "Lumen",
        lumen_text::TextStyle {
            font_size: 64.0,
            ..Default::default()
        },
        &[],
        None,
        lumen_text::TextAlign::Start,
    );
    let outlines = block.outlines();
    assert!(
        outlines.len() >= 5,
        "one path per glyph: {}",
        outlines.len()
    );
    assert!(outlines.iter().all(|p| p.elements().len() > 3));

    // And the app paints them (blue fill + orange stroke + green scale).
    let mut a = vectorial_text::main_app().run_headless(Size::new(560.0, 340.0));
    a.pump();
    let img = a.screenshot();
    let has = |pred: &dyn Fn(&[u8]) -> bool| img.pixels().chunks_exact(4).any(pred);
    assert!(has(&|p| p[2] > 180 && p[0] < 120), "blue fill painted");
    assert!(has(&|p| p[0] > 180 && p[2] < 120), "orange stroke painted");
}
