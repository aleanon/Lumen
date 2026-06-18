//! D1 — runtime motion: a spring-animated scalar usable directly in `build()`.
//!
//! [`spring`] returns the current value of a damped spring easing toward
//! `target`. Its state (origin, target, start time) lives in the reactive store
//! keyed by `name`, so it survives the from-scratch rebuild, hot reload, and a
//! tier-3 snapshot. While in flight it requests animation frames; once settled it
//! stops — idle stays at 0 frames, matching the event-driven model. Builds on the
//! `lumen-style` spring scheduler (04 §3).

use crate::element::BuildCx;
use lumen_style::anim::spring_progress;

/// Animate a scalar toward `target` with a spring (`stiffness`, `damping`).
/// Call every build with the latest `target`; returns the current value. The
/// first call rests at `target` (no jump); later target changes spring from the
/// current animated value.
pub fn spring(cx: &BuildCx, name: &str, target: f64, stiffness: f32, damping: f32) -> f64 {
    let rt = cx.runtime();
    let now = cx.now_ms();
    let from = cx.signal(&format!("{name}.spring.from"), || target);
    let tgt = cx.signal(&format!("{name}.spring.tgt"), || target);
    let t0 = cx.signal(&format!("{name}.spring.t0"), || now);

    let at = |from: f64, to: f64, start: f64| -> f64 {
        let p = spring_progress(((now - start) / 1000.0) as f32, stiffness, damping) as f64;
        from + (to - from) * p
    };

    // Re-target: restart from wherever the spring is right now.
    if (tgt.get(rt) - target).abs() > 1e-9 {
        let current = at(from.get(rt), tgt.get(rt), t0.get(rt));
        from.set(rt, current);
        tgt.set(rt, target);
        t0.set(rt, now);
    }

    let (f, to, start) = (from.get(rt), tgt.get(rt), t0.get(rt));
    let progress = spring_progress(((now - start) / 1000.0) as f32, stiffness, damping);
    if (progress - 1.0).abs() > 1e-3 {
        cx.animate(); // keep frames coming until it settles
    }
    at(f, to, start)
}

/// A reasonable default spring (snappy but smooth) for UI motion.
pub fn spring_default(cx: &BuildCx, name: &str, target: f64) -> f64 {
    spring(cx, name, target, 170.0, 26.0)
}
