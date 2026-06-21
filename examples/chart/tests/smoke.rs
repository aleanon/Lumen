use lumen_core::geometry::{Rect, Size};
use lumen_widgets::Headless;

fn bounds(a: &Headless, id: &str) -> Rect {
    fn find(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<Rect> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.bounds);
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    find(&a.semantics_doc().root, id).unwrap_or_else(|| panic!("no node {id}"))
}

#[test]
fn charts_render_with_labels_and_series() {
    let mut a = chart::main_app().run_headless(Size::new(600.0, 640.0));
    a.pump();

    // LeafWidget semantics make the charts addressable (accessible + testable).
    let t = a.semantics_json().to_string();
    assert!(t.contains("Line chart, 7 points"), "line chart semantics");
    assert!(t.contains("Bar chart, 4 bars"), "bar chart semantics");
    assert!(
        t.contains("Active users") && t.contains("Revenue"),
        "panel titles"
    );

    let img = a.screenshot();
    // Teal bars (high green+blue, low red) and the blue line/area both paint.
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[1] > 150 && p[2] > 130 && p[0] < 120),
        "teal bars painted"
    );
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[2] > 180 && p[0] < 140 && p[1] > 110 && p[1] < 200),
        "blue line painted"
    );
}

#[test]
fn leaf_flexes_to_panel_width() {
    // The constraint-aware leaf measure keeps an explicit `width: 100%`, so the
    // chart fills its panel instead of collapsing to its intrinsic default.
    // Panel content width ≈ 540 − 2·28 (card pad) − 2·16 (panel pad) ≈ 452.
    let mut a = chart::main_app().run_headless(Size::new(600.0, 640.0));
    a.pump();
    let w = bounds(&a, "line-chart").width();
    assert!(
        w > 420.0,
        "leaf flexed to panel width (got {w}, intrinsic default would be ~360)"
    );
}
