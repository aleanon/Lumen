//! A.2 (docs/plan-retained-pipeline.md): `.lss` layout properties reach
//! taffy — styles resolve inline in `build_node` *before* layout, so
//! `width`/`height`/`padding`/`margin`/`gap`/`display`/`flex-direction`
//! from the stylesheet are real, not parse-only.

use kurbo::Size;
use lumen_widgets::{col, row, widgets, App};

#[test]
fn lss_width_and_height_size_a_box() {
    // The sized node is a *container* — text-bearing nodes derive their
    // height from the glyphs (the documented text-height gotcha applies to
    // `.lss` heights exactly as it does to element-level ones).
    let mut h = App::new(|_cx| col![col![widgets::text("x")].id("box")])
        .stylesheet("#box { width: 120px; height: 48px; }")
        .run_headless(Size::new(400.0, 300.0));
    h.pump();
    let b = h.node_bounds_by_id("box").expect("laid out");
    assert_eq!(b.width(), 120.0, "lss width honored: {b:?}");
    assert_eq!(b.height(), 48.0, "lss height honored: {b:?}");
    h.assert_view_coherent();
}

#[test]
fn lss_gap_and_direction_space_children() {
    let mut h = App::new(|_cx| {
        col![
            widgets::button("A", |_| {}).id("a"),
            widgets::button("B", |_| {}).id("b"),
        ]
        .id("wrap")
    })
    .stylesheet(
        "#wrap { flex-direction: row; gap: 10px; } \
         #a { width: 50px; height: 20px; } \
         #b { width: 50px; height: 20px; }",
    )
    .run_headless(Size::new(400.0, 300.0));
    h.pump();
    let a = h.node_bounds_by_id("a").unwrap();
    let b = h.node_bounds_by_id("b").unwrap();
    // `flex-direction: row` overrode the column container; `gap` spaced them.
    assert_eq!(a.y0, b.y0, "row direction from .lss: {a:?} vs {b:?}");
    assert_eq!(b.x0 - a.x1, 10.0, "gap from .lss: {a:?} vs {b:?}");
    h.assert_view_coherent();
}

#[test]
fn lss_padding_insets_content_and_wraps_text() {
    // A text child inside a sized, padded box: the box obeys the stylesheet,
    // and the text (given an explicit .lss width) wraps to it.
    let mut h = App::new(|_cx| {
        col![widgets::text("a somewhat long label that should wrap to the styled width").id("t")]
            .id("box")
    })
    .stylesheet("#box { padding: 12px; } #t { width: 100px; }")
    .run_headless(Size::new(400.0, 300.0));
    h.pump();
    let bx = h.node_bounds_by_id("box").unwrap();
    let t = h.node_bounds_by_id("t").unwrap();
    assert_eq!(t.x0 - bx.x0, 12.0, "padding insets the child: {bx:?} {t:?}");
    assert_eq!(t.width(), 100.0, "text wraps at the .lss width: {t:?}");
    assert!(
        t.height() > 20.0,
        "wrapped to multiple lines at 100px: {t:?}"
    );
    h.assert_view_coherent();
}

#[test]
fn element_style_still_wins_when_sheet_is_silent() {
    // No stylesheet rule for the node: the element's own LayoutStyle is
    // untouched by the pre-layout resolution pass.
    let mut h = App::new(|_cx| {
        let mut e = widgets::button("Hi", |_| {}).id("plain");
        e.style.width = lumen_layout::Dim::px(77.0);
        col![e]
    })
    .stylesheet("#other { width: 200px; }")
    .run_headless(Size::new(400.0, 300.0));
    h.pump();
    let b = h.node_bounds_by_id("plain").unwrap();
    assert_eq!(b.width(), 77.0, "element style preserved: {b:?}");
    h.assert_view_coherent();
}

#[test]
fn hovered_state_layout_rule_relayouts() {
    // The A.2 risk note made explicit: a `:hovered` layout rule works and
    // re-enters the normal rebuild path on pointer motion.
    use lumen_core::events::{Event, PointerEvent};
    use lumen_widgets::center;

    let mut h = App::new(|_cx| col![widgets::button("Hi", |_| {}).id("g")])
        .stylesheet("#g { width: 60px; } #g:hovered { width: 90px; }")
        .run_headless(Size::new(400.0, 300.0));
    h.pump();
    assert_eq!(h.node_bounds_by_id("g").unwrap().width(), 60.0);
    let p = center(h.node_bounds_by_id("g").unwrap());
    h.inject(Event::PointerMove(PointerEvent::at(p)));
    h.pump();
    assert_eq!(
        h.node_bounds_by_id("g").unwrap().width(),
        90.0,
        "hover layout rule applied"
    );
    h.assert_view_coherent();
}
