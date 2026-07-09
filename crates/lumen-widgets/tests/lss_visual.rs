//! B.3a (docs/plan-remediation-2026-07.md): `.lss` `opacity` renders —
//! previously parsed and applied into the computed style but ignored by
//! paint. A half-opacity fill composites with what's beneath it.

use kurbo::Size;
use lumen_layout::Dim;
use lumen_widgets::{col, widgets, App, Element};

fn box_with(id: &str) -> Element {
    let mut e: Element = widgets::button("", |_| {}).id(id);
    e.style.width = Dim::px(100.0);
    e
}

#[test]
fn lss_opacity_composites_the_subtree() {
    let sheet = "#half { background: #ff0000ff; opacity: 0.5; } \
                 #full { background: #ff0000ff; }";
    let mut h = App::new(|_cx| col![box_with("half"), box_with("full")])
        .stylesheet(sheet)
        .run_headless(Size::new(300.0, 200.0));
    h.pump();

    let half = h.node_bounds_by_id("half").unwrap();
    let full = h.node_bounds_by_id("full").unwrap();
    let shot = h.screenshot();
    let ph = shot.pixel(half.center().x as u32, half.center().y as u32);
    let pf = shot.pixel(full.center().x as u32, full.center().y as u32);

    assert_eq!(pf[0], 255, "opaque box is fully red: {pf:?}");
    assert!(pf[1] < 40, "opaque box has no green: {pf:?}");
    // Half red composited over the light window background: still fully red
    // in R (both layers saturate it), but G/B climb toward the background.
    assert!(
        ph[1] > pf[1] + 60,
        "half-opacity lets the background through: {ph:?} vs {pf:?}"
    );
    h.assert_view_coherent();
}
