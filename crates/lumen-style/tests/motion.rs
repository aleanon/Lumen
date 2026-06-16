//! T6.4: shared-element transitions (time + gesture driven) and choreography.
use kurbo::Rect;
use lumen_style::anim::Easing;
use lumen_style::motion::{lerp_rect, SharedElement, Timeline, Track};

#[test]
fn shared_element_morphs_bounds() {
    let se = SharedElement {
        from: Rect::new(0.0, 0.0, 10.0, 10.0),
        to: Rect::new(100.0, 50.0, 140.0, 90.0),
        duration_ms: 200.0,
        easing: Easing::Linear,
    };
    assert_eq!(se.bounds_at(0.0), se.from);
    assert_eq!(se.bounds_at(200.0), se.to);
    assert!(se.settled(200.0) && !se.settled(100.0));
    // Halfway (linear) is the midpoint.
    let mid = se.bounds_at(100.0);
    assert!((mid.x0 - 50.0).abs() < 1e-6 && (mid.x1 - 75.0).abs() < 1e-6);
    // Gesture-driven gives the same midpoint at fraction 0.5.
    assert_eq!(se.bounds_at_fraction(0.5), mid);
}

#[test]
fn choreography_staggers_tracks() {
    let tl = Timeline {
        tracks: vec![
            Track {
                delay_ms: 0.0,
                duration_ms: 100.0,
                from: 0.0,
                to: 1.0,
                easing: Easing::Linear,
            },
            Track {
                delay_ms: 100.0,
                duration_ms: 100.0,
                from: 0.0,
                to: 1.0,
                easing: Easing::Linear,
            },
        ],
    };
    assert_eq!(tl.duration_ms(), 200.0);
    // At t=50: track0 halfway, track1 not started.
    let v = tl.values(50.0);
    assert!((v[0] - 0.5).abs() < 1e-6 && v[1] == 0.0);
    // At t=150: track0 done, track1 halfway.
    let v = tl.values(150.0);
    assert_eq!(v[0], 1.0);
    assert!((v[1] - 0.5).abs() < 1e-6);
    assert!(!tl.settled(150.0) && tl.settled(200.0));
}

#[test]
fn rect_lerp_and_spring() {
    let m = lerp_rect(
        Rect::new(0.0, 0.0, 0.0, 0.0),
        Rect::new(10.0, 20.0, 30.0, 40.0),
        0.25,
    );
    assert_eq!((m.x1, m.y1), (7.5, 10.0));
    // A spring approaches its target over time.
    let early = lumen_style::motion::spring(0.02, 170.0, 26.0);
    let late = lumen_style::motion::spring(1.0, 170.0, 26.0);
    assert!(early < late && (late - 1.0).abs() < 0.05);
}
