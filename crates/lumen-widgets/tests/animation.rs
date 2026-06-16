//! E8.9: deterministic virtual-clock animation/timer requests. The UI declares
//! what it wants (continuous animation, or a one-shot wake) during build; the
//! host reads it back to schedule frames. No wall clock is involved.
use lumen_core::geometry::Size;
use lumen_widgets::{widgets, App};

#[test]
fn idle_app_wants_no_frames() {
    let h = App::new(|_| widgets::text("x")).run_headless(Size::new(64.0, 64.0));
    assert!(!h.is_animating(), "a static UI does not animate");
    assert_eq!(h.next_deadline(), None, "idle UI costs no frames");
}

#[test]
fn animate_requests_a_frame_now() {
    let h = App::new(|cx| {
        cx.animate();
        widgets::text("x")
    })
    .run_headless(Size::new(64.0, 64.0));
    assert!(h.is_animating());
    // A continuous animation wants the next frame immediately (<= now).
    assert_eq!(h.next_deadline(), Some(h.now_ms()));
}

#[test]
fn animation_value_is_a_pure_function_of_the_clock() {
    // A counter that reads the clock: identical clock ⇒ identical frame.
    let app = App::new(|cx| {
        cx.animate();
        widgets::text(format!("t={}", cx.now_ms() as i64)).id("label")
    });
    let mut h = app.run_headless(Size::new(120.0, 40.0));
    assert!(h.semantics_json().to_string().contains("t=0"));
    h.advance(1000.0);
    assert!(
        h.semantics_json().to_string().contains("t=1000"),
        "advancing the clock advances the rendered value deterministically"
    );
}

#[test]
fn wake_in_schedules_one_frame_then_clears() {
    // A delayed reveal: "waiting" until 500ms, then "done" — driven only by time.
    let app = App::new(|cx| {
        let done = cx.now_ms() >= 500.0;
        if !done {
            cx.wake_in(500.0 - cx.now_ms());
        }
        widgets::text(if done { "done" } else { "waiting" }).id("status")
    });
    let mut h = app.run_headless(Size::new(120.0, 40.0));
    assert_eq!(h.next_deadline(), Some(500.0), "one-shot wake scheduled");
    assert!(h.semantics_json().to_string().contains("waiting"));

    h.advance(500.0);
    assert!(
        h.semantics_json().to_string().contains("done"),
        "the reveal happened at the deadline"
    );
    assert_eq!(
        h.next_deadline(),
        None,
        "the wake is not re-emitted once it has passed — idle again"
    );
}
