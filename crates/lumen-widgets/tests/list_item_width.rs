//! A `VirtualList` row spans the list, whatever it is made of.
//!
//! Pinning `left: 0` and `right: 0` on the item stretches a *container* row,
//! which is why the fix looked complete: `DataGrid` and every consumer row is a
//! container. It does not stretch a **text** row — a text-bearing element's
//! measure fixes its own width — so `|i| widgets::text(…)`, the most obvious
//! way to write a list, shrink-wrapped to its glyphs and everything to the
//! right of the label fell through the row on a tap.
//!
//! A definite width resolves before measure runs, so it lands where the insets
//! cannot; it is applied only when the caller left the width `Auto`, so an item
//! that sizes itself still wins.

use kurbo::Size;
use lumen_core::events::{Event, PointerEvent};
use lumen_core::geometry::Point;
use lumen_core::state::Signal;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, BuildCx, Element, VirtualList};
use std::rc::Rc;

const W: f64 = 300.0;
const VIEWPORT: f64 = 200.0;

/// A list whose rows are bare text — the shape that shrink-wrapped.
#[test]
fn a_bare_text_row_spans_the_list() {
    let build = |cx: &mut BuildCx| -> Element {
        let vl = VirtualList::new(cx, "vl", 100, 20.0, VIEWPORT, |i| {
            widgets::text(format!("row {i}")).id(format!("row-{i}"))
        });
        let mut root = widgets::column(vec![vl.into()]);
        root.style.width = Dim::px(W as f32);
        root.id("root")
    };
    let mut h = App::new(build).run_headless(Size::new(W, VIEWPORT));
    h.pump();

    let b = h.node_bounds_by_id("row-3").expect("row 3 is mounted");
    assert_eq!(
        (b.x0, b.x1),
        (0.0, W),
        "a text row must span the list, not its glyphs ({b:?})"
    );
}

/// The consequence that matters: the whole row is tappable, not just the label.
#[test]
fn the_full_width_of_a_text_row_is_tappable() {
    let build = |cx: &mut BuildCx| -> Element {
        let hits: Signal<u32> = cx.signal("hits", || 0);
        let vl = VirtualList::new(cx, "vl", 100, 20.0, VIEWPORT, move |i| {
            let mut row = widgets::text(format!("row {i}"));
            row.on_click = Some(Rc::new(move |rt| hits.update(rt, |v| *v += 1)));
            row.id(format!("row-{i}"))
        });
        let mut root = widgets::column(vec![vl.into()]);
        root.style.width = Dim::px(W as f32);
        root.id("root")
    };
    let mut h = App::new(build).run_headless(Size::new(W, VIEWPORT));
    h.pump();

    // Far right of the label, which is ~42 px wide.
    let p = Point::new(250.0, 70.0);
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();

    let hits: Signal<u32> = h.runtime().signal("hits", || 0);
    assert_eq!(
        hits.get(h.runtime()),
        1,
        "the empty space right of the label belongs to the row"
    );
}

/// An item that sizes itself keeps its width — the stretch fills in a default,
/// it does not override a decision.
#[test]
fn an_item_that_sets_its_own_width_keeps_it() {
    let build = |cx: &mut BuildCx| -> Element {
        let vl = VirtualList::new(cx, "vl", 100, 20.0, VIEWPORT, |i| {
            let mut row = widgets::column(vec![widgets::text(format!("row {i}"))]);
            row.style.width = Dim::px(120.0);
            row.id(format!("row-{i}"))
        });
        let mut root = widgets::column(vec![vl.into()]);
        root.style.width = Dim::px(W as f32);
        root.id("root")
    };
    let mut h = App::new(build).run_headless(Size::new(W, VIEWPORT));
    h.pump();

    let b = h.node_bounds_by_id("row-3").expect("row 3 is mounted");
    assert_eq!(b.x1 - b.x0, 120.0, "the caller's width wins ({b:?})");
}

/// Rows tile the list with no gap between them. `VirtualList` sets each item's
/// height to `item_height`, and text lowering used to overwrite it with the
/// glyph height — so a 24 px-pitch list of text rows laid out 21 px rows with a
/// 3 px dead strip between each pair, taking no taps and painting no background.
#[test]
fn text_rows_tile_the_list_with_no_dead_strip() {
    const PITCH: f64 = 24.0;
    let build = |cx: &mut BuildCx| -> Element {
        let vl = VirtualList::new(cx, "vl", 100, PITCH, VIEWPORT, |i| {
            widgets::text(format!("row {i}")).id(format!("row-{i}"))
        });
        let mut root = widgets::column(vec![vl.into()]);
        root.style.width = Dim::px(W as f32);
        root.id("root")
    };
    let mut h = App::new(build).run_headless(Size::new(W, VIEWPORT));
    h.pump();

    for i in 0..4 {
        let b = h
            .node_bounds_by_id(&format!("row-{i}"))
            .unwrap_or_else(|| panic!("row {i} is mounted"));
        assert_eq!(b.height(), PITCH, "row {i} must fill its pitch ({b:?})");
        // The next row starts exactly where this one ends.
        let n = h
            .node_bounds_by_id(&format!("row-{}", i + 1))
            .expect("next row");
        assert_eq!(n.y0, b.y1, "no gap between rows {i} and {}", i + 1);
    }
}

/// The consequence: a tap in the strip that used to belong to nobody now lands
/// on the row it looks like it belongs to.
#[test]
fn a_tap_near_the_bottom_edge_of_a_row_hits_that_row() {
    const PITCH: f64 = 24.0;
    let build = |cx: &mut BuildCx| -> Element {
        let hits: Signal<u32> = cx.signal("hits", || 0);
        let vl = VirtualList::new(cx, "vl", 100, PITCH, VIEWPORT, move |i| {
            let mut row = widgets::text(format!("row {i}"));
            row.on_click = Some(Rc::new(move |rt| hits.update(rt, |v| *v += 1)));
            row.id(format!("row-{i}"))
        });
        let mut root = widgets::column(vec![vl.into()]);
        root.style.width = Dim::px(W as f32);
        root.id("root")
    };
    let mut h = App::new(build).run_headless(Size::new(W, VIEWPORT));
    h.pump();

    // Row 2 spans 48..72; 70.5 sits in the band the glyphs never covered.
    let p = Point::new(150.0, 70.5);
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();

    let hits: Signal<u32> = h.runtime().signal("hits", || 0);
    assert_eq!(hits.get(h.runtime()), 1, "the bottom of a row is the row");
}
