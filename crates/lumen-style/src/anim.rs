//! The animation scheduler (04 §3 motion): transitions, keyframes, springs.
//!
//! Driven by a clock; tests advance a virtual clock and sample values
//! frame-by-frame. Once every animation settles the scheduler is idle (no more
//! frames — the event-driven contract, 01 §1.5). Reduced-motion clamps
//! durations to 0 unless forced.

use lumen_core::Color;

/// An interpolatable animated value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimValue {
    /// A scalar (px, opacity, …).
    Number(f64),
    /// A color (interpolated in Oklab, ADR-017).
    Color(Color),
}

impl AnimValue {
    fn lerp(self, to: AnimValue, t: f32) -> AnimValue {
        if t <= 0.0 {
            return self;
        }
        if t >= 1.0 {
            return to;
        }
        match (self, to) {
            (AnimValue::Number(a), AnimValue::Number(b)) => {
                AnimValue::Number(a + (b - a) * t as f64)
            }
            (AnimValue::Color(a), AnimValue::Color(b)) => AnimValue::Color(a.lerp_oklab(b, t)),
            // Mismatched kinds snap to the target.
            (_, b) => b,
        }
    }
}

/// An easing function (04 §3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    /// Constant rate.
    Linear,
    /// `ease`.
    Ease,
    /// `ease-in`.
    EaseIn,
    /// `ease-out`.
    EaseOut,
    /// `ease-in-out`.
    EaseInOut,
    /// `cubic-bezier(x1, y1, x2, y2)`.
    CubicBezier(f32, f32, f32, f32),
    /// `spring(stiffness, damping)`.
    Spring(f32, f32),
}

impl Easing {
    /// Map normalized time `t ∈ [0,1]` → eased progress (non-spring easings).
    pub fn apply(self, t: f32) -> f32 {
        let (x1, y1, x2, y2) = match self {
            Easing::Linear => return t,
            Easing::Ease => (0.25, 0.1, 0.25, 1.0),
            Easing::EaseIn => (0.42, 0.0, 1.0, 1.0),
            Easing::EaseOut => (0.0, 0.0, 0.58, 1.0),
            Easing::EaseInOut => (0.42, 0.0, 0.58, 1.0),
            Easing::CubicBezier(a, b, c, d) => (a, b, c, d),
            Easing::Spring(..) => return t, // handled by the transition
        };
        cubic_bezier(x1, y1, x2, y2, t)
    }
}

/// Step response of a damped spring from 0 → 1 at `elapsed` seconds.
pub fn spring_progress(elapsed: f32, stiffness: f32, damping: f32) -> f32 {
    if elapsed <= 0.0 {
        return 0.0;
    }
    let w0 = stiffness.max(0.0001).sqrt();
    let zeta = damping / (2.0 * w0);
    let t = elapsed;
    if zeta < 1.0 {
        // underdamped
        let wd = w0 * (1.0 - zeta * zeta).sqrt();
        1.0 - (-zeta * w0 * t).exp() * ((wd * t).cos() + (zeta * w0 / wd) * (wd * t).sin())
    } else {
        // critically/over-damped: use the critically-damped form
        1.0 - (-w0 * t).exp() * (1.0 + w0 * t)
    }
}

/// One animated property: from `from` to `to` over `duration_ms` (springs run
/// until settled). Times are absolute milliseconds on the scheduler's clock.
#[derive(Clone, Debug)]
pub struct Transition {
    from: AnimValue,
    to: AnimValue,
    start_ms: f64,
    duration_ms: f64,
    easing: Easing,
}

impl Transition {
    fn elapsed_frac(&self, now: f64) -> f32 {
        if self.duration_ms <= 0.0 {
            return 1.0;
        }
        (((now - self.start_ms) / self.duration_ms) as f32).clamp(0.0, 1.0)
    }

    /// The value at clock time `now`.
    pub fn value_at(&self, now: f64) -> AnimValue {
        let progress = match self.easing {
            Easing::Spring(stiff, damp) => {
                let secs = ((now - self.start_ms).max(0.0) / 1000.0) as f32;
                spring_progress(secs, stiff, damp)
            }
            e => e.apply(self.elapsed_frac(now)),
        };
        self.from.lerp(self.to, progress)
    }

    /// Whether the animation has settled at `now`.
    pub fn finished(&self, now: f64) -> bool {
        match self.easing {
            Easing::Spring(stiff, damp) => {
                let secs = ((now - self.start_ms).max(0.0) / 1000.0) as f32;
                (spring_progress(secs, stiff, damp) - 1.0).abs() < 0.001 && secs > 0.0
            }
            _ => now >= self.start_ms + self.duration_ms,
        }
    }
}

/// Schedules and samples animations against a single clock. Generic over the
/// target id type so callers key by node/property.
pub struct Scheduler<Id: Eq + std::hash::Hash + Clone> {
    active: std::collections::HashMap<Id, Transition>,
    /// When true, non-spring durations clamp to 0 (OS reduced-motion).
    pub reduced_motion: bool,
}

impl<Id: Eq + std::hash::Hash + Clone> Default for Scheduler<Id> {
    fn default() -> Self {
        Scheduler {
            active: std::collections::HashMap::new(),
            reduced_motion: false,
        }
    }
}

impl<Id: Eq + std::hash::Hash + Clone> Scheduler<Id> {
    /// A fresh scheduler.
    pub fn new() -> Self {
        Scheduler::default()
    }

    /// Start (or replace) an animation for `id`.
    pub fn start(
        &mut self,
        id: Id,
        from: AnimValue,
        to: AnimValue,
        duration_ms: f64,
        easing: Easing,
        now: f64,
    ) {
        let duration_ms = if self.reduced_motion && !matches!(easing, Easing::Spring(..)) {
            0.0
        } else {
            duration_ms
        };
        self.active.insert(
            id,
            Transition {
                from,
                to,
                start_ms: now,
                duration_ms,
                easing,
            },
        );
    }

    /// The current value for `id`, if animating.
    pub fn value(&self, id: &Id, now: f64) -> Option<AnimValue> {
        self.active.get(id).map(|t| t.value_at(now))
    }

    /// Sample all active animations at `now`.
    pub fn sample(&self, now: f64) -> Vec<(Id, AnimValue)> {
        self.active
            .iter()
            .map(|(id, t)| (id.clone(), t.value_at(now)))
            .collect()
    }

    /// Remove finished animations; call after sampling each frame.
    pub fn prune(&mut self, now: f64) {
        self.active.retain(|_, t| !t.finished(now));
    }

    /// Whether nothing is animating (the scheduler requests no more frames).
    pub fn is_idle(&self) -> bool {
        self.active.is_empty()
    }
}

/// Evaluate a cubic-bezier easing `y` at parameter `t` (Newton iteration on x).
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let bez = |a: f32, b: f32, s: f32| {
        let s1 = 1.0 - s;
        3.0 * s1 * s1 * s * a + 3.0 * s1 * s * s * b + s * s * s
    };
    // Solve x(s) = t for s.
    let mut s = t;
    for _ in 0..8 {
        let x = bez(x1, x2, s) - t;
        let dx = 3.0 * (1.0 - s).powi(2) * x1
            + 6.0 * (1.0 - s) * s * (x2 - x1)
            + 3.0 * s * s * (1.0 - x2);
        if dx.abs() < 1e-6 {
            break;
        }
        s -= x / dx;
        s = s.clamp(0.0, 1.0);
    }
    bez(y1, y2, s)
}
