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

// --- M.3: motion wiring — routes + gestures ----------------------------------

/// Retained shared-element morph (M.3). Call every build with THIS build's
/// target bounds for the element; whenever the target changes (typically a
/// route switch moved the shared element's home), the returned rect animates
/// from wherever the previous morph currently is to the new target on the
/// virtual clock — the wiring `lumen_style::motion::SharedElement` left to
/// the app. First sighting rests at `target` (no mount flash).
pub fn shared_bounds(
    cx: &BuildCx,
    name: &str,
    target: kurbo::Rect,
    duration_ms: f64,
) -> kurbo::Rect {
    #[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
    struct Morph {
        started: bool,
        from: (f64, f64, f64, f64),
        to: (f64, f64, f64, f64),
        start_ms: f64,
    }
    fn pack(r: kurbo::Rect) -> (f64, f64, f64, f64) {
        (r.x0, r.y0, r.x1, r.y1)
    }
    fn unpack(t: (f64, f64, f64, f64)) -> kurbo::Rect {
        kurbo::Rect::new(t.0, t.1, t.2, t.3)
    }
    let now = cx.now_ms();
    let sig = cx.signal::<Morph>(name, Morph::default);
    let m = sig.get(cx.runtime());
    let cur = |m: &Morph| {
        let t = if duration_ms <= 0.0 {
            1.0
        } else {
            (((now - m.start_ms) / duration_ms).clamp(0.0, 1.0)) as f32
        };
        lumen_style::motion::lerp_rect(
            unpack(m.from),
            unpack(m.to),
            lumen_style::Easing::EaseInOut.apply(t),
        )
    };
    if !m.started {
        sig.set(
            cx.runtime(),
            Morph {
                started: true,
                from: pack(target),
                to: pack(target),
                start_ms: now,
            },
        );
        return target;
    }
    if unpack(m.to) != target {
        // Retarget from the CURRENT blended position (smooth interruption).
        let from = cur(&m);
        sig.set(
            cx.runtime(),
            Morph {
                started: true,
                from: pack(from),
                to: pack(target),
                start_ms: now,
            },
        );
        cx.animate();
        return from;
    }
    let r = cur(&m);
    if now - m.start_ms < duration_ms {
        cx.animate();
    }
    r
}

/// Route-transition progress (M.3, the `nav::Router` integration): 0→1 on
/// the virtual clock after every change of `route`, then rests at 1. Drive
/// screen cross-fades/slides from it; pair with [`shared_bounds`] for the
/// hero element.
pub fn route_progress(cx: &BuildCx, name: &str, route: &str, duration_ms: f64) -> f32 {
    #[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
    struct Tr {
        started: bool,
        route: String,
        start_ms: f64,
    }
    let now = cx.now_ms();
    let sig = cx.signal::<Tr>(name, Tr::default);
    let t = sig.get(cx.runtime());
    if !t.started || t.route != route {
        let first = !t.started;
        let route = route.to_string();
        sig.set(
            cx.runtime(),
            Tr {
                started: true,
                route,
                // The very first build rests settled; later changes animate.
                start_ms: if first { now - duration_ms } else { now },
            },
        );
        if !first {
            cx.animate();
            return 0.0;
        }
        return 1.0;
    }
    if duration_ms <= 0.0 {
        return 1.0;
    }
    let p = ((now - t.start_ms) / duration_ms).clamp(0.0, 1.0) as f32;
    if p < 1.0 {
        cx.animate();
    }
    p
}

/// Gesture fraction (M.3): attach to any element — dragging across it maps
/// the pointer's horizontal position to `{name}` ∈ [0,1] in the store.
/// Read it back with [`drag_fraction`] and feed
/// `SharedElement::bounds_at_fraction` / [`shared_bounds`] targets — the
/// gesture *is* the timeline.
pub fn drag_surface(cx: &BuildCx, name: &str, mut el: crate::Element) -> crate::Element {
    let sig = cx.signal::<f64>(name, || 0.0);
    el.on_drag = Some(std::rc::Rc::new(move |rt, fx, _fy, _pos| {
        sig.set(rt, fx.clamp(0.0, 1.0));
    }));
    el
}

/// The current gesture fraction written by [`drag_surface`].
pub fn drag_fraction(cx: &BuildCx, name: &str) -> f64 {
    cx.signal::<f64>(name, || 0.0).get(cx.runtime())
}
