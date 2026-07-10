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

#[test]
fn lss_shadow_paints_behind_the_box() {
    // A hard (zero-blur) red shadow offset 8px right+down: the pixel just
    // outside the box's bottom-right corner is shadow-only.
    let sheet = "#sh { background: #0000ffff; shadow: 8px 8px 0 #ff0000ff; }";
    let mut h = App::new(|_cx| col![box_with("sh")])
        .stylesheet(sheet)
        .run_headless(Size::new(300.0, 200.0));
    h.pump();

    let b = h.node_bounds_by_id("sh").unwrap();
    let shot = h.screenshot();
    let inside = shot.pixel(b.center().x as u32, b.center().y as u32);
    let shadow = shot.pixel(b.x1 as u32 + 4, b.y1 as u32 + 4);
    assert!(inside[2] > 200, "box itself is blue: {inside:?}");
    assert!(
        shadow[0] > 200 && shadow[2] < 60,
        "offset corner shows the red shadow: {shadow:?}"
    );
    h.assert_view_coherent();
}

#[test]
fn lss_visibility_hidden_removes_paint_hits_and_semantics_but_keeps_space() {
    let sheet = "#gone { visibility: hidden; }";
    let mut h = App::new(|_cx| col![box_with("gone"), box_with("below")])
        .stylesheet(sheet)
        .run_headless(Size::new(300.0, 200.0));
    h.pump();

    // Layout space kept: the second box does NOT move up to y=0.
    let below = h.node_bounds_by_id("below").unwrap();
    assert!(
        below.y0 > 10.0,
        "hidden box keeps its layout space: {below:?}"
    );

    // Not painted: the hidden box's area shows the window background.
    let gone = h.node_bounds_by_id("gone").unwrap();
    let shot = h.screenshot();
    let p = shot.pixel(gone.center().x as u32, gone.center().y as u32);
    let bg = shot.pixel(295, 195);
    assert_eq!(p, bg, "hidden box paints nothing: {p:?} vs bg {bg:?}");

    // Not in semantics.
    let sem = h.semantics_json().to_string();
    assert!(!sem.contains("gone"), "hidden subtree leaves semantics");
    assert!(sem.contains("below"), "visible sibling stays");
}

#[test]
fn lss_multi_value_border_radius_rounds_per_corner() {
    // Only the bottom-left corner is rounded (40px): its corner pixel is
    // outside the shape while the top-left corner pixel stays filled.
    let sheet = "#rr { background: #ff0000ff; border-radius: 0 0 0 40px; }";
    let mut h = App::new(|_cx| {
        let mut e = box_with("rr");
        e.style.height = Dim::px(80.0);
        col![e]
    })
    .stylesheet(sheet)
    .run_headless(Size::new(300.0, 200.0));
    h.pump();

    let b = h.node_bounds_by_id("rr").unwrap();
    let shot = h.screenshot();
    let tl = shot.pixel(b.x0 as u32 + 2, b.y0 as u32 + 2);
    let bl = shot.pixel(b.x0 as u32 + 2, b.y1 as u32 - 3);
    assert!(
        tl[0] > 200 && tl[1] < 60,
        "square top-left corner is filled red: {tl:?}"
    );
    assert!(
        bl[1] > 150,
        "40px bottom-left corner is cut (window background shows): {bl:?}"
    );
    h.assert_view_coherent();
}
