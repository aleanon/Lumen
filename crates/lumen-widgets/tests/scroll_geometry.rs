//! A scroll surface never resizes its content.
//!
//! `Scrollable`'s viewport is a flex container, and flex's default
//! `align_items: Stretch` was clamping the content column to the viewport's
//! cross size. A column of fixed-height rows taller than the viewport therefore
//! had its rows *flex-shrunk* to fit — and because scrolling moves the column
//! with a negative `margin-top`, the box grew as you scrolled and the rows grew
//! back toward their real height. Row pitch visibly changed under the pointer.

use kurbo::{Rect, Size};
use lumen_core::events::{Event, Modifiers, WheelEvent};
use lumen_core::geometry::{Point, Vec2};
use lumen_core::semantics::SemanticsNode;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, BuildCx, Element, Headless, Scrollable};

const ROW_H: f64 = 30.0;
const ROWS: usize = 60;
const VIEWPORT: f64 = 200.0;

fn rect_id(n: &SemanticsNode, id: &str) -> Option<Rect> {
    if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
        return Some(n.bounds);
    }
    n.children.iter().find_map(|c| rect_id(c, id))
}

fn app() -> Headless {
    let build = |cx: &mut BuildCx| -> Element {
        let rows: Vec<Element> = (0..ROWS)
            .map(|i| {
                let mut r = widgets::text(format!("row {i}")).id(format!("row-{i}"));
                r.style.height = Dim::px(ROW_H as f32);
                r
            })
            .collect();
        widgets::column(vec![Scrollable::new(
            cx,
            "sc",
            VIEWPORT,
            ROWS as f64 * ROW_H,
            rows,
        )
        .into()])
    };
    let mut h = App::new(build).run_headless(Size::new(300.0, VIEWPORT));
    h.pump();
    h
}

fn row_height(h: &Headless, i: usize) -> f64 {
    let b = rect_id(&h.semantics_doc().root, &format!("row-{i}")).expect("row is mounted");
    b.y1 - b.y0
}

#[test]
fn rows_keep_their_height_at_every_scroll_offset() {
    let mut h = app();
    assert_eq!(row_height(&h, 0), ROW_H, "unscrolled");
    for step in 1..=6 {
        h.inject(Event::Wheel(WheelEvent {
            pos: Point::new(150.0, 100.0),
            delta: Vec2::new(0.0, 400.0),
            modifiers: Modifiers::empty(),
        }));
        h.pump();
        assert_eq!(
            row_height(&h, 0),
            ROW_H,
            "row height changed after {step} scroll steps"
        );
    }
}

/// The consequence: the row you can see is the row you can click. With the
/// rows squeezed, the pitch on screen no longer matched the pitch the caller
/// asked for, so hit targets drifted from where they were painted.
#[test]
fn row_pitch_matches_the_requested_item_height() {
    let h = app();
    let a = rect_id(&h.semantics_doc().root, "row-0").unwrap();
    let b = rect_id(&h.semantics_doc().root, "row-1").unwrap();
    assert_eq!(b.y0 - a.y0, ROW_H);
}

/// The bar is addressable, which is what lets a drag survive the rebuild each
/// scroll step causes — and what lets an agent or a test grab it at all.
#[test]
fn the_overlay_scrollbar_has_a_stable_id() {
    let h = app();
    assert!(
        rect_id(&h.semantics_doc().root, "sc-scrollbar").is_some(),
        "the scroll surface named `sc` exposes `#sc-scrollbar`"
    );
}

/// A drag on the bar survives the rebuild that scrolling causes.
///
/// Every scroll step rebuilds, and a drag is re-resolved by stable id with the
/// raw node index as a fallback — so the bar, which had no id, was riding on an
/// index the rebuild was free to renumber. This drives the bar the way the
/// pointer does and checks the grab still moves it several rebuilds later.
#[test]
fn a_scrollbar_drag_survives_the_rebuilds_it_causes() {
    use lumen_core::events::PointerEvent;
    use lumen_core::state::Signal;
    let mut h = app();
    let bar = rect_id(&h.semantics_doc().root, "sc-scrollbar").expect("the bar");
    let x = (bar.x0 + bar.x1) / 2.0;
    let offset: Signal<f64> = h.runtime().signal("sc", || 0.0f64);

    h.inject(Event::PointerDown(PointerEvent::at(Point::new(
        x,
        bar.y0 + 10.0,
    ))));
    h.pump();
    let mut seen = Vec::new();
    for step in 1..=8 {
        let y = bar.y0 + 10.0 + step as f64 * 40.0;
        h.inject(Event::PointerMove(PointerEvent::at(Point::new(x, y))));
        h.pump();
        seen.push(offset.get(h.runtime()));
    }
    assert!(
        seen.windows(2).all(|w| w[1] >= w[0]) && seen.last() > seen.first(),
        "the grab kept moving the offset across rebuilds: {seen:?}"
    );
    // And back up again, still holding.
    h.inject(Event::PointerMove(PointerEvent::at(Point::new(
        x,
        bar.y0 + 20.0,
    ))));
    h.pump();
    assert!(
        offset.get(h.runtime()) < *seen.last().unwrap(),
        "the same grab scrolls back up"
    );
}
