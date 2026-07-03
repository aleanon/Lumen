//! The grid grows as you scroll (no fixed extent): only the viewport is
//! materialized (a few hundred nodes), the wheel scrolls and *extends* it, a
//! window resize reveals more rows, and dragging a header edge resizes that
//! column/row — all staying coherent.

use lumen_core::events::{Event, Modifiers, PointerButton, PointerEvent, PointerKind, WheelEvent};
use lumen_core::geometry::{Point, Size, Vec2};
use lumen_widgets::Headless;

fn pe(x: f64, y: f64) -> PointerEvent {
    PointerEvent {
        pos: Point::new(x, y),
        button: PointerButton::Left,
        pointer: PointerKind::Mouse,
        modifiers: Default::default(),
        click_count: 1,
    }
}

fn wheel(h: &mut Headless, dx: f64, dy: f64) {
    h.inject(Event::Wheel(WheelEvent {
        pos: Point::new(400.0, 400.0),
        delta: Vec2::new(dx, dy),
        modifiers: Modifiers::empty(),
    }));
    h.pump();
}

fn info(h: &Headless) -> String {
    h.semantics_json().to_string()
}

#[test]
fn virtualized_bounded_and_scrolls() {
    let mut h = datagrid::main_app().run_headless(Size::new(1000.0, 700.0));
    let n = h.pump().node_count;
    // A few hundred cells materialized over an unbounded grid — not billions.
    assert!(
        (300..2000).contains(&n),
        "viewport node count bounded, got {n}"
    );
    assert!(info(&h).contains("rows 1–"), "starts at row 1");
    h.assert_view_coherent();

    // Wheel down → the visible row range moves and the node count stays bounded.
    wheel(&mut h, 0.0, 4000.0);
    let after = h.pump().node_count;
    assert!(!info(&h).contains("rows 1–"), "scrolled off the top");
    assert!(
        (300..2000).contains(&after),
        "still bounded after scroll, got {after}"
    );
    h.assert_view_coherent();

    // Wheel back to the top.
    wheel(&mut h, 0.0, -9000.0);
    assert!(info(&h).contains("rows 1–"), "scrolled back to row 1");
    h.assert_view_coherent();
}

#[test]
fn scrolls_past_the_old_u16_ceiling() {
    let mut h = datagrid::main_app().run_headless(Size::new(1000.0, 700.0));
    h.pump();
    // Scroll far past where a u16 (65535) grid would have run out of rows.
    let deep = 70_000.0 * 24.0; // ≈ row 70000, well beyond u16::MAX
    wheel(&mut h, 0.0, deep);
    let sy: lumen_core::state::Signal<f64> = h.runtime().signal("sy", || 0.0);
    assert!(
        sy.get(h.runtime()) >= 65535.0 * 24.0,
        "scrolled into u32 territory"
    );
    let n = h.pump().node_count;
    assert!(
        (300..2000).contains(&n),
        "still bounded that far out, got {n}"
    );
    assert!(!info(&h).contains("rows 1–"));
    h.assert_view_coherent();
}

#[test]
fn window_resize_reveals_more_rows() {
    let mut h = datagrid::main_app().run_headless(Size::new(1000.0, 500.0));
    let small = h.pump().node_count;
    h.assert_view_coherent();

    // A taller, wider window materializes strictly more cells (more rows/cols fit).
    h.resize(Size::new(1400.0, 1000.0));
    let large = h.pump().node_count;
    assert!(
        large > small,
        "resize revealed more cells: {small} -> {large}"
    );
    h.assert_view_coherent();
}

#[test]
fn dragging_a_column_header_edge_resizes_it() {
    let mut h = datagrid::main_app().run_headless(Size::new(1000.0, 700.0));
    h.pump();

    // Column 2's right border ≈ x = HDR_W(48) + 3*DW(64) = 240; the handle sits in
    // the header band, just below the toolbar (y ≈ 46..72).
    let border_x = 48.0 + 3.0 * 64.0;
    // The handle for the *next* column (cx-3) marks where column 3 starts; it must
    // shift right once column 2 widens — proof the grid actually reflowed.
    let before = h.node_bounds_by_id("cx-3").expect("cx-3 handle exists").x0;

    h.inject(Event::PointerDown(pe(border_x, 58.0)));
    h.inject(Event::PointerMove(pe(border_x + 60.0, 58.0)));
    h.inject(Event::PointerUp(pe(border_x + 60.0, 58.0)));
    h.pump();

    // Column 2 now has a width override (wider than the 64px default)...
    let cw: lumen_core::state::Signal<Vec<(u32, f64)>> = h.runtime().signal("cw", Vec::new);
    let over = cw.get(h.runtime());
    let w = over.iter().find(|(k, _)| *k == 2).map(|(_, w)| *w);
    assert!(
        w.is_some_and(|w| w > 70.0),
        "column 2 widened by the drag, override = {over:?}"
    );
    // ...and everything to its right actually moved over.
    let after = h.node_bounds_by_id("cx-3").expect("cx-3 handle exists").x0;
    assert!(
        after > before + 40.0,
        "column 3 reflowed right: {before} -> {after}"
    );
    h.assert_view_coherent();
}
