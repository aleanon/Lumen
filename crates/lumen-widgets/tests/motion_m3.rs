//! M.3: motion wiring — shared-element morphs on target change, route
//! transitions track the Router, gestures drive fractions. All on the
//! virtual clock, all state in the store (survives rebuild/snapshot).

use kurbo::{Rect, Size};
use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
use lumen_core::geometry::Point;
use lumen_widgets::nav::Router;
use lumen_widgets::{motion, widgets, App};

#[test]
fn shared_bounds_morphs_on_target_change_and_settles() {
    let mut h = App::new(|cx| {
        let expanded = cx.signal("expanded", || false);
        let target = if expanded.get(cx.runtime()) {
            Rect::new(0.0, 0.0, 200.0, 200.0)
        } else {
            Rect::new(0.0, 0.0, 100.0, 100.0)
        };
        let r = motion::shared_bounds(cx, "hero", target, 200.0);
        widgets::text(format!("w={:.0}", r.width())).id("w")
    })
    .run_headless(Size::new(300.0, 100.0));
    h.pump();
    assert!(
        h.semantics_json().to_string().contains("w=100"),
        "rests at first target"
    );

    let expanded = h.runtime().signal("expanded", || false);
    expanded.set(h.runtime(), true);
    h.pump();
    // Mid-flight: strictly between the two targets.
    h.advance_clock(100.0);
    h.pump();
    let t = h.semantics_json().to_string();
    let w: f64 = t
        .split("w=")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(w > 100.0 && w < 200.0, "mid-morph: {w}");
    // Settled.
    h.advance_clock(200.0);
    h.pump();
    assert!(h.semantics_json().to_string().contains("w=200"));
}

#[test]
fn route_progress_animates_each_navigation() {
    let mut h = App::new(|cx| {
        let router = cx.signal("router", || Router::new("home"));
        let route = router.get(cx.runtime()).current().to_string();
        let p = motion::route_progress(cx, "nav-anim", &route, 200.0);
        widgets::text(format!("route={route} p={p:.2}")).id("t")
    })
    .run_headless(Size::new(300.0, 100.0));
    h.pump();
    assert!(
        h.semantics_json().to_string().contains("p=1.00"),
        "first build settled"
    );

    let router = h.runtime().signal("router", || Router::new("home"));
    router.update(h.runtime(), |r| r.navigate("details"));
    h.pump();
    let t = h.semantics_json().to_string();
    assert!(
        t.contains("route=details") && t.contains("p=0.00"),
        "restarts: {t}"
    );
    h.advance_clock(100.0);
    h.pump();
    let t = h.semantics_json().to_string();
    assert!(
        t.contains("p=0.5") || t.contains("p=0.4") || t.contains("p=0.6"),
        "mid: {t}"
    );
    h.advance_clock(200.0);
    h.pump();
    assert!(h.semantics_json().to_string().contains("p=1.00"));
}

#[test]
fn drag_surface_writes_the_fraction() {
    let mut h = App::new(|cx| {
        let f = motion::drag_fraction(cx, "sheet");
        let mut surface = widgets::column(vec![widgets::text(format!("f={f:.2}")).id("f")]);
        surface.style.width = lumen_layout::Dim::px(200.0);
        surface.style.height = lumen_layout::Dim::px(50.0);
        surface.background = Some(lumen_core::Color::srgb8(40, 40, 60, 255));
        motion::drag_surface(cx, "sheet", surface).id("surface")
    })
    .run_headless(Size::new(300.0, 100.0));
    h.pump();
    assert!(h.semantics_json().to_string().contains("f=0.00"));

    // Press at x=20, drag to x=150 (75% across the 200px surface).
    let ev = |x: f64| PointerEvent {
        pos: Point::new(x, 25.0),
        button: PointerButton::Left,
        pointer: PointerKind::Mouse,
        modifiers: Default::default(),
        click_count: 1,
    };
    h.inject(Event::PointerDown(ev(20.0)));
    h.inject(Event::PointerMove(ev(150.0)));
    h.pump();
    let t = h.semantics_json().to_string();
    assert!(
        t.contains("f=0.75") || t.contains("f=0.7"),
        "gesture drove the fraction: {t}"
    );
}
