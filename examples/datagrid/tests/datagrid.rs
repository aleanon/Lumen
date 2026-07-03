//! The grid grows as you scroll (no fixed extent): only the viewport is
//! materialized (a few hundred nodes), the wheel scrolls and *extends* it, a
//! window resize reveals more rows, and dragging a header edge resizes that
//! column/row — all staying coherent.

use lumen_core::events::{
    Event, Modifiers, PointerButton, PointerEvent, PointerKind, TextInputEvent, WheelEvent,
};
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

fn click(h: &mut Headless, x: f64, y: f64) {
    h.inject(Event::PointerDown(pe(x, y)));
    h.inject(Event::PointerUp(pe(x, y)));
    h.pump();
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
fn clicking_a_cell_lets_you_type_and_commits() {
    let mut h = datagrid::main_app().run_headless(Size::new(1000.0, 700.0));
    h.pump();

    // Click cell B2 (col idx 1: x≈144; row idx 1: y≈108) → it becomes the editor.
    click(&mut h, 144.0, 108.0);
    let editing: lumen_core::state::Signal<Option<(u32, u32)>> =
        h.runtime().signal("editing", || None);
    assert_eq!(editing.get(h.runtime()), Some((1, 1)), "B2 is being edited");

    // Type; then commit by clicking another cell.
    h.inject(Event::TextInput(TextInputEvent { text: "x".into() }));
    h.pump();
    click(&mut h, 144.0, 132.0); // B3

    let edits: lumen_core::state::Signal<Vec<(u32, u32, String)>> =
        h.runtime().signal("edits", Vec::new);
    let v = edits.get(h.runtime());
    let b2 = v
        .iter()
        .find(|(r, c, _)| (*r, *c) == (1, 1))
        .map(|(_, _, s)| s.clone());
    assert_eq!(
        b2.as_deref(),
        Some("21x"),
        "typed into B2 committed, got {v:?}"
    );
    h.assert_view_coherent();
}

#[test]
fn shift_wheel_scrolls_horizontally() {
    let mut h = datagrid::main_app().run_headless(Size::new(1000.0, 700.0));
    h.pump();
    // Shift + vertical wheel moves the horizontal offset, not the vertical one.
    h.inject(Event::Wheel(WheelEvent {
        pos: Point::new(400.0, 400.0),
        delta: Vec2::new(0.0, 600.0),
        modifiers: Modifiers::SHIFT,
    }));
    h.pump();
    let sx: lumen_core::state::Signal<f64> = h.runtime().signal("sx", || 0.0);
    let sy: lumen_core::state::Signal<f64> = h.runtime().signal("sy", || 0.0);
    assert!(sx.get(h.runtime()) > 100.0, "shift+wheel scrolled right");
    assert_eq!(sy.get(h.runtime()), 0.0, "vertical offset unchanged");
    assert!(
        info(&h).contains("cols") && !info(&h).contains("cols A–"),
        "columns scrolled"
    );
    h.assert_view_coherent();
}

#[test]
fn ctrl_wheel_zooms() {
    let mut h = datagrid::main_app().run_headless(Size::new(1000.0, 700.0));
    h.pump();
    let zoom: lumen_core::state::Signal<f64> = h.runtime().signal("zoom", || 1.0);
    assert_eq!(zoom.get(h.runtime()), 1.0);

    // Ctrl + wheel-up zooms in; a plain wheel does not.
    h.inject(Event::Wheel(WheelEvent {
        pos: Point::new(400.0, 400.0),
        delta: Vec2::new(0.0, -200.0),
        modifiers: Modifiers::CTRL,
    }));
    h.pump();
    assert!(zoom.get(h.runtime()) > 1.1, "ctrl+wheel zoomed in");
    assert!(info(&h).contains('%'), "toolbar shows the zoom level");
    h.assert_view_coherent();
}

#[test]
fn dragging_the_vertical_thumb_scrolls() {
    let mut h = datagrid::main_app().run_headless(Size::new(1000.0, 700.0));
    h.pump();
    let thumb = h.node_bounds_by_id("vthumb").expect("vertical thumb");
    let (cx, cy) = (
        thumb.x0 + thumb.width() / 2.0,
        thumb.y0 + thumb.height() / 2.0,
    );

    h.inject(Event::PointerDown(pe(cx, cy)));
    h.inject(Event::PointerMove(pe(cx, cy + 200.0)));
    h.inject(Event::PointerUp(pe(cx, cy + 200.0)));
    h.pump();

    let sy: lumen_core::state::Signal<f64> = h.runtime().signal("sy", || 0.0);
    assert!(sy.get(h.runtime()) > 100.0, "thumb drag scrolled down");
    assert!(!info(&h).contains("rows 1–"));
    h.assert_view_coherent();
}

#[test]
fn resizing_a_row_grows_the_cell_not_the_gap() {
    // Regression: a text-bearing cell used to ignore its explicit height and
    // size to the glyphs, so a resized row left an empty gap instead of a taller
    // cell. Assert the row-header *box* actually grows to the new height.
    let mut h = datagrid::main_app().run_headless(Size::new(1000.0, 700.0));
    h.pump();
    let before = h.node_bounds_by_id("rh-2").expect("row header 2").height();
    assert!(before < 30.0, "default row height, got {before}");

    // Row 2's bottom border ≈ window y = TOOLBAR(46)+HDR_H(26)+3*DH(24) = 144.
    h.inject(Event::PointerDown(pe(24.0, 144.0)));
    h.inject(Event::PointerMove(pe(24.0, 169.0)));
    h.inject(Event::PointerMove(pe(24.0, 194.0)));
    h.inject(Event::PointerUp(pe(24.0, 194.0)));
    h.pump();

    let after = h.node_bounds_by_id("rh-2").expect("row header 2").height();
    assert!(
        after > before + 30.0,
        "the resized row's cell grew to fill the slot: {before} -> {after}"
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
