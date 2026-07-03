//! The grid scales to thousands of nodes via zoom (rows GC'd on zoom-in), stays
//! coherent throughout (fine-grained memoization + glyph-run reuse), and a cell
//! click highlights that cell.

use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
use lumen_core::geometry::{Point, Size};
use lumen_widgets::Headless;

fn click_at(h: &mut Headless, x: f64, y: f64) {
    let pe = PointerEvent {
        pos: Point::new(x, y),
        button: PointerButton::Left,
        pointer: PointerKind::Mouse,
        modifiers: Default::default(),
        click_count: 1,
    };
    h.inject(Event::PointerDown(pe));
    h.inject(Event::PointerUp(pe));
    h.pump();
}

#[test]
fn scales_to_thousands_of_nodes_and_stays_coherent() {
    let mut h = datagrid::main_app().run_headless(Size::new(1000.0, 700.0));
    let base = h.pump().node_count;
    assert!(base > 300, "default grid has hundreds of nodes, got {base}");
    h.assert_view_coherent();

    // Zoom all the way out → thousands of nodes (each `−` adds rows + cols).
    for _ in 0..3 {
        h.invoke_action("#zoom-out", "click").unwrap();
    }
    let big = h.pump().node_count;
    assert!(
        big > 2500,
        "zoomed-out grid has thousands of nodes, got {big}"
    );
    h.assert_view_coherent();

    // Zoom back in → fewer nodes (removed rows swept by the F5 GC), still coherent.
    for _ in 0..4 {
        h.invoke_action("#zoom-in", "click").unwrap();
    }
    let small = h.pump().node_count;
    assert!(small < big, "zoom-in shed nodes: {small} < {big}");
    h.assert_view_coherent();
}

#[test]
fn clicking_a_cell_highlights_it() {
    let mut h = datagrid::main_app().run_headless(Size::new(1000.0, 700.0));
    h.pump();
    let hot = |h: &mut Headless| {
        // The clicked-cell highlight colour (#2b4c8f) appears in the frame.
        h.screenshot()
            .pixels()
            .chunks_exact(4)
            .any(|p| p[0].abs_diff(0x2b) < 6 && p[1].abs_diff(0x4c) < 6 && p[2].abs_diff(0x8f) < 6)
    };
    assert!(!hot(&mut h), "no cell highlighted before a click");
    // Click inside the grid (below the header/bar) — hits a data cell.
    click_at(&mut h, 150.0, 150.0);
    assert!(hot(&mut h), "the clicked cell is highlighted");
    h.assert_view_coherent();
}
