//! T3.5 acceptance (part 1): gesture synthesis — tap/double-tap/long-press/
//! pan/pinch are recognized from synthesized touch streams.

use kurbo::Point;
use lumen_core::events::GestureKind;
use lumen_core::gesture::{GestureRecognizer, TouchPhase, LONG_PRESS_MS};

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

#[test]
fn tap_is_recognized() {
    let mut r = GestureRecognizer::new();
    assert!(r.feed(0.0, TouchPhase::Begin, 1, p(10.0, 10.0)).is_empty());
    let g = r.feed(50.0, TouchPhase::End, 1, p(11.0, 10.0));
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].kind, GestureKind::Tap);
    assert_eq!(g[0].pointers, 1);
}

#[test]
fn double_tap_is_recognized() {
    let mut r = GestureRecognizer::new();
    r.feed(0.0, TouchPhase::Begin, 1, p(10.0, 10.0));
    assert_eq!(
        r.feed(40.0, TouchPhase::End, 1, p(10.0, 10.0))[0].kind,
        GestureKind::Tap
    );
    r.feed(100.0, TouchPhase::Begin, 2, p(10.0, 10.0));
    let g = r.feed(140.0, TouchPhase::End, 2, p(10.0, 10.0));
    assert_eq!(g[0].kind, GestureKind::DoubleTap);
}

#[test]
fn long_press_fires_after_threshold() {
    let mut r = GestureRecognizer::new();
    r.feed(0.0, TouchPhase::Begin, 1, p(10.0, 10.0));
    assert!(r.tick(100.0).is_empty(), "too early");
    let g = r.tick(LONG_PRESS_MS + 1.0);
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].kind, GestureKind::LongPress);
    // It fires only once.
    assert!(r.tick(LONG_PRESS_MS + 50.0).is_empty());
}

#[test]
fn movement_suppresses_long_press_and_yields_pan() {
    let mut r = GestureRecognizer::new();
    r.feed(0.0, TouchPhase::Begin, 1, p(10.0, 10.0));
    let g = r.feed(20.0, TouchPhase::Move, 1, p(40.0, 10.0));
    assert_eq!(g.len(), 1);
    match g[0].kind {
        GestureKind::Pan { delta, velocity } => {
            assert_eq!(delta.x, 30.0);
            assert!(velocity.x > 0.0);
        }
        other => panic!("expected pan, got {other:?}"),
    }
    // Long-press must not fire once moved.
    assert!(r.tick(LONG_PRESS_MS + 100.0).is_empty());
}

#[test]
fn two_pointers_yield_pinch_scale() {
    let mut r = GestureRecognizer::new();
    // Two fingers 100px apart.
    r.feed(0.0, TouchPhase::Begin, 1, p(0.0, 0.0));
    r.feed(0.0, TouchPhase::Begin, 2, p(100.0, 0.0));
    // Spread to 200px apart -> scale 2.0.
    let g = r.feed(16.0, TouchPhase::Move, 2, p(200.0, 0.0));
    assert_eq!(g.len(), 1);
    match g[0].kind {
        GestureKind::Pinch { scale, .. } => assert!((scale - 2.0).abs() < 1e-6, "scale {scale}"),
        other => panic!("expected pinch, got {other:?}"),
    }
    assert_eq!(g[0].pointers, 2);
}
