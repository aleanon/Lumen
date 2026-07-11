//! B.5 (docs/plan-remediation-2026-07.md): `transition:` plays. Paint-tier
//! properties (background/color/opacity/border-radius) interpolate between
//! computed values on nodes with stable ids, driven by the virtual clock;
//! reduced motion snaps; the hover restyle path animates too.

use kurbo::Size;
use lumen_core::events::{Event, PointerEvent};
use lumen_core::state::Signal;
use lumen_widgets::{center, col, widgets, App, BuildCx, Element};

fn box_el(warn: bool) -> Element {
    let mut e: Element = widgets::button("", |_| {}).id("b");
    e.style.width = lumen_layout::Dim::px(100.0);
    if warn {
        e = e.class("warn");
    }
    e
}

fn probe(h: &mut lumen_widgets::Headless) -> [u8; 4] {
    let b = h.node_bounds_by_id("b").unwrap();
    let shot = h.screenshot();
    shot.pixel(b.center().x as u32, b.center().y as u32)
}

const SHEET: &str = "#b { background: #0000ffff; transition: background 100ms linear; } \
                     #b.warn { background: #ff0000ff; }";

#[test]
fn class_flip_interpolates_background() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let warn = cx.signal("warn", || false);
        col![box_el(warn.get(cx.runtime()))]
    })
    .stylesheet(SHEET)
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    let p0 = probe(&mut h);
    assert!(p0[2] > 200 && p0[0] < 60, "starts blue: {p0:?}");

    let warn: Signal<bool> = h.runtime().signal("warn", || false);
    warn.set(h.runtime(), true);
    h.pump();
    assert!(h.is_time_driven(), "transition running");

    h.advance_clock(50.0);
    h.pump();
    let mid = probe(&mut h);
    assert!(
        mid[0] > 60 && mid[0] < 230 && mid[2] > 30 && mid[2] < 230,
        "halfway blend, neither pure: {mid:?}"
    );

    h.advance_clock(60.0);
    h.pump();
    let end = probe(&mut h);
    assert!(end[0] > 200 && end[2] < 60, "landed on red: {end:?}");
    assert!(!h.is_time_driven(), "settled");
    h.assert_view_coherent();
}

#[test]
fn reduced_motion_snaps() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let warn = cx.signal("warn", || false);
        col![box_el(warn.get(cx.runtime()))]
    })
    .stylesheet(SHEET)
    .run_headless(Size::new(300.0, 200.0));
    h.set_reduced_motion(true);
    h.pump();

    let warn: Signal<bool> = h.runtime().signal("warn", || false);
    warn.set(h.runtime(), true);
    h.pump();
    let p = probe(&mut h);
    assert!(p[0] > 200 && p[2] < 60, "instant red: {p:?}");
    assert!(!h.is_time_driven(), "nothing to settle");
}

#[test]
fn hover_transition_animates_through_the_restyle_path() {
    let sheet = "#b { background: #0000ffff; transition: background 100ms linear; } \
                 #b:hovered { background: #ff0000ff; }";
    let mut h = App::new(|_cx| col![box_el(false)])
        .stylesheet(sheet)
        .run_headless(Size::new(300.0, 200.0));
    h.pump();

    let p = center(h.node_bounds_by_id("b").unwrap());
    h.inject(Event::PointerMove(PointerEvent::at(p)));
    h.pump();
    assert_eq!(
        h.last_change()["kind"],
        "restyle",
        "hover stayed restyle-only: {:?}",
        h.last_change()
    );
    h.advance_clock(50.0);
    h.pump();
    let mid = probe(&mut h);
    assert!(
        mid[0] > 60 && mid[0] < 230,
        "hover color mid-blend: {mid:?}"
    );
    h.advance_clock(60.0);
    h.pump();
    let end = probe(&mut h);
    assert!(end[0] > 200, "hover target reached: {end:?}");
    h.assert_view_coherent();
}
