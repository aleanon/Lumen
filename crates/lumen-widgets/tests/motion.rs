//! D1: the runtime spring eases toward a (changing) target and settles, driving
//! frames only while in flight.

use lumen_core::geometry::Size;
use lumen_widgets::{motion, App, BuildCx, Element};

/// An app whose readout is a spring animating toward a clickable target.
fn app() -> App {
    App::new(|cx: &mut BuildCx| {
        let target = cx.signal("target", || 0.0f64);
        let t = target.get(cx.runtime());
        let v = motion::spring(cx, "anim", t, 170.0, 26.0);
        Element {
            role: lumen_core::semantics::Role::Button,
            label: format!("{v:.1}"),
            on_click: Some(std::rc::Rc::new(move |rt| target.set(rt, 100.0))),
            background: Some(lumen_core::Color::srgb8(0x20, 0x80, 0xf0, 0xff)),
            style: lumen_layout::LayoutStyle {
                width: lumen_layout::Dim::pct(1.0),
                height: lumen_layout::Dim::pct(1.0),
                ..Default::default()
            },
            ..Element::default()
        }
        .id("box")
    })
}

fn value(h: &lumen_widgets::Headless) -> f64 {
    fn find(n: &lumen_core::semantics::SemanticsNode) -> Option<f64> {
        if n.id.as_ref().map(|i| i.as_str()) == Some("box") {
            return n.label.parse().ok();
        }
        n.children.iter().find_map(find)
    }
    find(&h.semantics_doc().root).unwrap()
}

#[test]
fn spring_eases_then_settles() {
    let mut h = app().run_headless(Size::new(80.0, 60.0));
    h.pump();
    assert_eq!(value(&h), 0.0, "rests at the initial target, no jump");

    // Retarget to 100 via the click handler.
    use lumen_core::events::{Event, PointerEvent};
    let pe = PointerEvent::at(kurbo::Point::new(40.0, 30.0));
    h.inject(Event::PointerDown(pe));
    h.inject(Event::PointerUp(pe));
    h.pump();

    // Mid-flight: moved off 0 but not yet at 100.
    h.advance(80.0);
    let mid = value(&h);
    assert!(mid > 0.0 && mid < 100.0, "spring is animating, got {mid}");
    assert!(h.is_animating(), "requests frames while in flight");

    // Give it time to settle.
    for _ in 0..120 {
        h.advance(16.0);
    }
    let settled = value(&h);
    assert!(
        (settled - 100.0).abs() < 1.0,
        "settles at target, got {settled}"
    );
    assert!(!h.is_animating(), "stops requesting frames once settled");
}
