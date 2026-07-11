//! W.4 (docs/plan-remediation-2026-07.md): `#[state_registry]` — stored
//! trait objects (02 §4). `Box<dyn Trait>` values live in the reactive store,
//! serialize as `{ "type": tag, "value": fields }`, and rebuild their vtables
//! on restore through the per-trait registry. Unregistered tags surface as
//! `W0002` drops via the lenient restore.
#![cfg(feature = "snapshot")]

use lumen_core::state::{Runtime, Signal};
use serde::{Deserialize, Serialize};

#[lumen_macros::state_registry]
trait Shape: std::fmt::Debug {
    fn area(&self) -> f64;
}

#[derive(Debug, Serialize, Deserialize)]
struct Circle {
    r: f64,
}
impl Shape for Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * self.r * self.r
    }
}
lumen_core::stored_type!(Circle as "circle");

#[derive(Debug, Serialize, Deserialize)]
struct Square {
    side: f64,
}
impl Shape for Square {
    fn area(&self) -> f64 {
        self.side * self.side
    }
}
lumen_core::stored_type!(Square as "square");

fn register_all() {
    register_shape::<Circle>("circle");
    register_shape::<Square>("square");
}

#[test]
fn trait_objects_round_trip_through_a_snapshot() {
    register_all();
    let rt = Runtime::new();
    let shapes: Signal<Vec<Box<dyn Shape>>> = rt.signal("shapes", || {
        vec![
            Box::new(Circle { r: 1.0 }) as Box<dyn Shape>,
            Box::new(Square { side: 3.0 }),
        ]
    });
    let snap = rt.snapshot();

    // Fresh runtime: the signal re-creates from the staged snapshot; the
    // registry rebuilds the vtables from the tags.
    let rt2 = Runtime::new();
    rt2.load_pending(snap);
    let restored: Signal<Vec<Box<dyn Shape>>> = rt2.signal("shapes", Vec::new);
    let diags = rt2.finish_restore();
    assert!(diags.is_empty(), "clean restore: {diags:?}");
    restored.with(&rt2, |v| {
        assert_eq!(v.len(), 2);
        assert!((v[0].area() - std::f64::consts::PI).abs() < 1e-9);
        assert!((v[1].area() - 9.0).abs() < 1e-9);
    });
    let _ = shapes;
}

#[test]
fn unregistered_tag_drops_with_w0002() {
    register_all();
    let rt = Runtime::new();
    let _: Signal<Vec<Box<dyn Shape>>> = rt.signal("solo", || {
        vec![Box::new(Circle { r: 2.0 }) as Box<dyn Shape>]
    });
    let mut snap_json = serde_json::to_value(rt.snapshot()).unwrap();
    // Corrupt the tag to something never registered.
    let s = snap_json.to_string().replace("circle", "hexagon");
    snap_json = serde_json::from_str(&s).unwrap();
    let snap: lumen_core::state::StateSnapshot = serde_json::from_value(snap_json).unwrap();

    let rt2 = Runtime::new();
    rt2.load_pending(snap);
    let restored: Signal<Vec<Box<dyn Shape>>> = rt2.signal("solo", Vec::new);
    let diags = rt2.finish_restore();
    assert!(
        diags.iter().any(|d| d.code == "W0002"),
        "unregistered tag reported as W0002: {diags:?}"
    );
    restored.with(&rt2, |v| assert!(v.is_empty(), "fell back to init()"));
}
