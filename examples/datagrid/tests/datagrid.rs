//! The grid is virtualized over a u16² space: only the viewport is materialized
//! (a few hundred nodes, not billions), the wheel scrolls it, and dragging a
//! header edge resizes that column/row — all staying coherent.

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

#[test]
fn virtualized_bounded_and_scrolls() {
    let mut h = datagrid::main_app().run_headless(Size::new(1000.0, 700.0));
    let n = h.pump().node_count;
    // A few hundred cells materialized over a 65535×65535 grid — not billions.
    assert!(
        (300..2000).contains(&n),
        "viewport node count bounded, got {n}"
    );
    assert!(h.semantics_json().to_string().contains("showing R1"));
    h.assert_view_coherent();

    // Wheel down a long way → scroll past the filled 100×100 block; the visible
    // row range moves and the node count stays bounded (virtualization).
    wheel(&mut h, 0.0, 4000.0);
    let after = h.pump().node_count;
    assert!(
        !h.semantics_json().to_string().contains("showing R1 "),
        "scrolled off the top"
    );
    assert!(
        (300..2000).contains(&after),
        "still bounded after scroll, got {after}"
    );
    h.assert_view_coherent();

    // Wheel back to the top.
    wheel(&mut h, 0.0, -9000.0);
    assert!(h.semantics_json().to_string().contains("showing R1"));
    h.assert_view_coherent();
}

#[test]
fn dragging_a_column_header_edge_resizes_it() {
    let mut h = datagrid::main_app().run_headless(Size::new(1000.0, 700.0));
    h.pump();

    // Column 2's right border ≈ x = HDR_W(48) + 3*DW(64) = 240; the handle sits in
    // the header band, just below the toolbar (y ≈ 46..76).
    let border_x = 48.0 + 3.0 * 64.0;
    h.inject(Event::PointerDown(pe(border_x, 58.0)));
    h.inject(Event::PointerMove(pe(border_x + 60.0, 58.0)));
    h.inject(Event::PointerUp(pe(border_x + 60.0, 58.0)));
    h.pump();

    // Column 2 now has a width override (wider than the 64px default).
    let cw: lumen_core::state::Signal<Vec<(u16, f64)>> = h.runtime().signal("cw", Vec::new);
    let over = cw.get(h.runtime());
    let w = over.iter().find(|(k, _)| *k == 2).map(|(_, w)| *w);
    assert!(
        w.is_some_and(|w| w > 70.0),
        "column 2 widened by the drag, override = {over:?}"
    );
    h.assert_view_coherent();
}
