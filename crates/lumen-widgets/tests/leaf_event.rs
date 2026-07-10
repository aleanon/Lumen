//! W.0 (ADR-W1, docs/plan-remediation-2026-07.md): custom leaves get an
//! `event()` hook — first refusal at the event's target, `Handled`
//! consumes (Element-level handlers skipped), `Ignored` falls through.

use kurbo::{Rect, Size};
use lumen_core::events::{Event, EventStatus, PointerEvent};
use lumen_core::semantics::Role;
use lumen_core::state::{Runtime, Signal};
use lumen_widgets::{center, col, widgets, App, LeafWidget};

/// Counts pointer-downs into a signal; `consume` decides Handled/Ignored.
struct Tally {
    hits: Signal<i64>,
    consume: bool,
}

impl LeafWidget for Tally {
    fn measure(&self, _available: Size) -> Size {
        Size::new(80.0, 24.0)
    }
    fn paint(&self, _frame: &mut lumen_render::canvas::Frame, _size: Size) {}
    fn semantics(&self) -> (Role, String) {
        (Role::Button, "tally".to_string())
    }
    fn event(&self, event: &Event, _bounds: Rect, rt: &Runtime) -> EventStatus {
        if matches!(event, Event::PointerDown(_)) {
            self.hits.update(rt, |v| *v += 1);
            if self.consume {
                return EventStatus::Handled;
            }
        }
        EventStatus::Ignored
    }
}

fn click(h: &mut lumen_widgets::Headless, id: &str) {
    let p = center(h.node_bounds_by_id(id).unwrap());
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
}

#[test]
fn handled_consumes_the_event() {
    let mut h = App::new(|cx| {
        let hits = cx.signal("hits", || 0i64);
        let clicks = cx.signal("clicks", || 0i64);
        let mut leaf = widgets::leaf(Tally {
            hits,
            consume: true,
        })
        .id("leaf");
        leaf.on_click = Some(std::rc::Rc::new(move |rt: &Runtime| {
            clicks.update(rt, |v| *v += 1)
        }));
        col![leaf]
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    click(&mut h, "leaf");

    let hits: Signal<i64> = h.runtime().signal("hits", || 0);
    let clicks: Signal<i64> = h.runtime().signal("clicks", || 0);
    assert_eq!(hits.get(h.runtime()), 1, "leaf saw the pointer-down");
    assert_eq!(
        clicks.get(h.runtime()),
        0,
        "Handled consumed it — on_click never fired"
    );
    h.assert_view_coherent();
}

#[test]
fn ignored_falls_through_to_element_handlers() {
    let mut h = App::new(|cx| {
        let hits = cx.signal("hits", || 0i64);
        let clicks = cx.signal("clicks", || 0i64);
        let mut leaf = widgets::leaf(Tally {
            hits,
            consume: false,
        })
        .id("leaf");
        leaf.on_click = Some(std::rc::Rc::new(move |rt: &Runtime| {
            clicks.update(rt, |v| *v += 1)
        }));
        col![leaf]
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    click(&mut h, "leaf");

    let hits: Signal<i64> = h.runtime().signal("hits", || 0);
    let clicks: Signal<i64> = h.runtime().signal("clicks", || 0);
    assert_eq!(hits.get(h.runtime()), 1, "leaf observed the event");
    assert_eq!(
        clicks.get(h.runtime()),
        1,
        "Ignored fell through — on_click fired too"
    );
    h.assert_view_coherent();
}
