//! T1.4 acceptance: frame-by-frame value assertions on a virtual clock, and an
//! idle-after-settle check (0 frames once animations finish).

use lumen_core::Color;
use lumen_style::anim::{spring_progress, AnimValue};
use lumen_style::{Easing, Scheduler};

fn num(v: AnimValue) -> f64 {
    match v {
        AnimValue::Number(n) => n,
        _ => panic!("not a number"),
    }
}

#[test]
fn linear_transition_frame_by_frame() {
    let mut s: Scheduler<u32> = Scheduler::new();
    s.start(
        1,
        AnimValue::Number(0.0),
        AnimValue::Number(100.0),
        100.0,
        Easing::Linear,
        0.0,
    );
    for (t, expect) in [
        (0.0, 0.0),
        (25.0, 25.0),
        (50.0, 50.0),
        (75.0, 75.0),
        (100.0, 100.0),
    ] {
        assert!(
            (num(s.value(&1, t).unwrap()) - expect).abs() < 1e-6,
            "at {t}ms"
        );
    }
}

#[test]
fn color_transition_interpolates_in_oklab() {
    let mut s: Scheduler<u32> = Scheduler::new();
    let a = Color::srgb8(0xff, 0x00, 0x00, 0xff);
    let b = Color::srgb8(0x00, 0x00, 0xff, 0xff);
    s.start(
        1,
        AnimValue::Color(a),
        AnimValue::Color(b),
        100.0,
        Easing::Linear,
        0.0,
    );
    // endpoints exact, midpoint distinct
    assert_eq!(s.value(&1, 0.0), Some(AnimValue::Color(a)));
    assert_eq!(s.value(&1, 100.0), Some(AnimValue::Color(b)));
    let mid = match s.value(&1, 50.0).unwrap() {
        AnimValue::Color(c) => c,
        _ => unreachable!(),
    };
    assert_ne!(mid.to_hex(), a.to_hex());
    assert_ne!(mid.to_hex(), b.to_hex());
}

#[test]
fn ease_is_monotonic_with_fixed_endpoints() {
    assert!((Easing::Ease.apply(0.0)).abs() < 1e-3);
    assert!((Easing::Ease.apply(1.0) - 1.0).abs() < 1e-3);
    let mut prev = -1.0;
    for i in 0..=10 {
        let y = Easing::EaseInOut.apply(i as f32 / 10.0);
        assert!(y >= prev - 1e-4, "non-monotonic at {i}");
        prev = y;
    }
}

#[test]
fn spring_settles_to_one() {
    // underdamped spring overshoots then settles toward 1
    let early = spring_progress(0.05, 170.0, 26.0);
    let late = spring_progress(2.0, 170.0, 26.0);
    assert!((0.0..1.2).contains(&early));
    assert!(
        (late - 1.0).abs() < 0.01,
        "spring should settle, got {late}"
    );
}

#[test]
fn idle_after_settle() {
    let mut s: Scheduler<u32> = Scheduler::new();
    s.start(
        1,
        AnimValue::Number(0.0),
        AnimValue::Number(1.0),
        100.0,
        Easing::Linear,
        0.0,
    );
    assert!(!s.is_idle(), "animating");
    s.prune(50.0);
    assert!(!s.is_idle(), "still animating at 50ms");
    s.prune(100.0);
    assert!(s.is_idle(), "settled at 100ms -> 0 frames");
}

#[test]
fn reduced_motion_is_instant() {
    let mut s: Scheduler<u32> = Scheduler::new();
    s.reduced_motion = true;
    s.start(
        1,
        AnimValue::Number(0.0),
        AnimValue::Number(100.0),
        1000.0,
        Easing::EaseInOut,
        0.0,
    );
    // jumps straight to the target
    assert!((num(s.value(&1, 0.0).unwrap()) - 100.0).abs() < 1e-6);
    s.prune(0.0);
    assert!(s.is_idle());
}
