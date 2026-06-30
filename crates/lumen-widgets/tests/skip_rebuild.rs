//! The runtime skips the rebuild when nothing that affects the frame changed
//! (idle/non-effecting pumps cost ~µs), while still rebuilding on real changes.

use kurbo::Size;
use lumen_widgets::{App, Element};
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn idle_pump_does_not_rebuild_a_static_ui() {
    let builds = Rc::new(Cell::new(0u32));
    let b = builds.clone();
    let mut h = App::new(move |_cx| {
        b.set(b.get() + 1);
        Element::text("static")
    })
    .run_headless(Size::new(100.0, 40.0));

    assert_eq!(builds.get(), 1, "one build at startup");
    h.pump();
    h.pump();
    assert_eq!(
        builds.get(),
        1,
        "idle pumps (no input/signal/time change) must not rebuild"
    );
}

#[test]
fn clock_read_rebuilds_only_when_the_clock_advances() {
    let builds = Rc::new(Cell::new(0u32));
    let b = builds.clone();
    let mut h = App::new(move |cx| {
        b.set(b.get() + 1);
        // Reading the clock marks the frame time-dependent.
        Element::text(format!("{}", cx.now_ms()))
    })
    .run_headless(Size::new(120.0, 40.0));

    assert_eq!(builds.get(), 1);
    h.pump();
    assert_eq!(
        builds.get(),
        1,
        "no clock advance → no rebuild even though the build reads the clock"
    );
    h.advance_clock(16.0);
    h.pump();
    assert_eq!(
        builds.get(),
        2,
        "clock advanced + build reads the clock → rebuild"
    );
}

#[test]
fn resize_forces_a_rebuild() {
    let builds = Rc::new(Cell::new(0u32));
    let b = builds.clone();
    let mut h = App::new(move |_cx| {
        b.set(b.get() + 1);
        Element::text("static")
    })
    .run_headless(Size::new(100.0, 40.0));

    assert_eq!(builds.get(), 1);
    h.resize(Size::new(200.0, 80.0));
    assert_eq!(
        builds.get(),
        2,
        "a resize re-lays-out (not a signal change)"
    );
}
