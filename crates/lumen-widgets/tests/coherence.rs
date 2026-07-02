//! F0: the coherence oracle + harness. The view must equal a fresh rebuild from
//! the same state (`incremental == rebuild_fresh`), and a settled `pump` must
//! leave the reactive graph quiescent. Trivially true under the full-rebuild
//! model; this is the guardrail F1/F2 build against.

use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
use lumen_core::geometry::{Point, Size};
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use lumen_widgets::{widgets, App, BuildCx, Element};

/// A centred button counting its clicks (label shows the count) — a click at the
/// window centre hits it.
fn counter() -> App {
    App::new(|cx: &mut BuildCx| {
        let count = cx.signal("count", || 0i64);
        let rt = cx.runtime();
        let btn = widgets::button("inc", move |rt| count.update(rt, |c| *c += 1)).id("inc");
        Element {
            role: lumen_core::semantics::Role::Group,
            label: format!("count={}", count.get(rt)),
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Dim::pct(1.0),
                height: Dim::pct(1.0),
                align_items: Some(Align::Center),
                justify_content: Some(Align::Center),
                ..LayoutStyle::default()
            },
            children: vec![btn],
            ..Element::default()
        }
    })
}

fn click_centre(h: &mut lumen_widgets::Headless) {
    let pe = PointerEvent {
        pos: Point::new(100.0, 100.0),
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
fn view_is_coherent_after_interaction() {
    let mut h = counter().run_headless(Size::new(200.0, 200.0));
    h.assert_view_coherent(); // clean from the initial build

    // Drive it; the live view must still match a fresh rebuild each time.
    for _ in 0..3 {
        click_centre(&mut h);
        h.assert_view_coherent();
    }
    assert!(h.semantics_json().to_string().contains("count=3"));
}

#[test]
fn pump_leaves_the_graph_quiescent() {
    // pump()'s debug_assert is the primary check; assert it explicitly too.
    let mut h = counter().run_headless(Size::new(200.0, 200.0));
    assert!(
        h.runtime().is_quiescent(),
        "quiescent after the initial build"
    );
    click_centre(&mut h);
    assert!(h.runtime().is_quiescent(), "quiescent after a click");
    h.pump(); // idle
    assert!(h.runtime().is_quiescent(), "quiescent after an idle pump");
}
