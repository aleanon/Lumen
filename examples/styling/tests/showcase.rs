//! The showcase builds and the `.lss` palette actually paints — distinct status
//! colours appear across the widgets.

use lumen_core::geometry::Size;

#[test]
fn themed_palette_renders() {
    let mut a = styling::main_app().run_headless(Size::new(760.0, 760.0));
    a.pump();
    let img = a.screenshot();
    let any = |pred: fn(&[u8]) -> bool| img.pixels().chunks_exact(4).any(pred);

    // info #2563eb, success #15a34a, danger #dc2626 — all driven by app.lss.
    assert!(
        any(|p| p[2] > 180 && p[0] < 120 && p[1] < 150),
        "info blue present"
    );
    assert!(
        any(|p| p[1] > 120 && p[0] < 120 && p[2] < 120),
        "success green present"
    );
    assert!(
        any(|p| p[0] > 180 && p[1] < 90 && p[2] < 90),
        "danger red present"
    );

    assert!(
        a.semantics_json().to_string().contains("Design System"),
        "heading present"
    );
}
