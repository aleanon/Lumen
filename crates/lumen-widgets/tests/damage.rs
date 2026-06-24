//! R2.5 — the damage/incremental-repaint contract.
//!
//! The retained frame must always equal a from-scratch full render: a small
//! state change repaints only a bounded *region* (not the whole frame, and not
//! nothing), an idle pump repaints *nothing* and reuses the frame, and the
//! incrementally-composited frame is byte-identical to a forced full repaint.

use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
use lumen_core::geometry::{Point, Size};
use lumen_core::semantics::SemanticsNode;
use lumen_render::Damage;
use lumen_widgets::{App, BuildCx, Button, Container, Element, Headless, Label};

fn build(cx: &mut BuildCx) -> Element {
    let rt = cx.runtime();
    let n = cx.signal("n", || 0i32);
    Container::new(vec![
        Label::new("Static header — never changes").into(),
        Label::new(format!("count: {}", n.get(rt)))
            .id("count")
            .into(),
        Button::new("inc")
            .id("inc")
            .on_press(move |rt| n.update(rt, |v| *v += 1))
            .into(),
    ])
    .gap(8.0)
    .padding(16.0)
    .into()
}

fn app() -> Headless {
    App::new(build).run_headless(Size::new(300.0, 200.0))
}

fn rect_id(n: &SemanticsNode, id: &str) -> Option<lumen_core::geometry::Rect> {
    if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
        return Some(n.bounds);
    }
    n.children.iter().find_map(|c| rect_id(c, id))
}

fn click(a: &mut Headless, id: &str) {
    let b = rect_id(&a.semantics_doc().root, id).unwrap_or_else(|| panic!("no #{id}"));
    let pe = PointerEvent {
        pos: Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0),
        button: PointerButton::Left,
        pointer: PointerKind::Mouse,
        modifiers: Default::default(),
        click_count: 1,
    };
    a.inject(Event::PointerDown(pe));
    a.inject(Event::PointerUp(pe));
    a.pump();
}

#[test]
fn idle_pump_paints_nothing_and_reuses_frame() {
    let mut a = app();
    // The first frame (construction) is a full paint; a no-op pump after it has
    // an identical display list, so nothing is repainted.
    let stats = a.pump();
    assert_eq!(
        stats.damage,
        Damage::None,
        "idle pump should be damage-free"
    );
    assert!(!stats.painted, "idle pump should not paint");

    let before = a.screenshot();
    a.pump();
    let after = a.screenshot();
    assert_eq!(
        before.pixels(),
        after.pixels(),
        "idle frame must be reused verbatim"
    );
}

#[test]
fn small_edit_damages_only_a_region() {
    let mut a = app();
    click(&mut a, "inc");
    match a.last_damage() {
        Damage::Region(r) => {
            // The damage must be a real sub-region, not the whole 300×200 frame.
            assert!(r.width() > 0.0 && r.height() > 0.0, "non-empty region");
            assert!(
                r.width() < 300.0 || r.height() < 200.0,
                "a one-label change must not damage the whole frame: {r:?}"
            );
        }
        other => panic!("expected a damage region, got {other:?}"),
    }
}

#[test]
fn incremental_frame_is_byte_identical_to_full_repaint() {
    let mut a = app();
    for _ in 0..5 {
        click(&mut a, "inc"); // incremental repaint of the changed region
        let incremental = a.screenshot();
        a.force_full_repaint(); // same state, repainted from scratch
        let full = a.screenshot();
        assert_eq!(
            incremental.pixels(),
            full.pixels(),
            "incremental composite must equal a full render"
        );
    }
}
