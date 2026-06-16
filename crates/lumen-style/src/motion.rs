//! Motion system (T6.4): shared-element transitions and choreography on top of
//! the [`crate::anim`] scheduler. Everything is a pure function of time (or a
//! gesture fraction), so motion is deterministic under the test virtual clock.

use crate::anim::{spring_progress, Easing};
use kurbo::Rect;

fn lerp(a: f64, b: f64, t: f32) -> f64 {
    a + (b - a) * t as f64
}

/// Interpolate a rectangle from `a` to `b` at progress `t ∈ [0,1]`.
pub fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::new(
        lerp(a.x0, b.x0, t),
        lerp(a.y0, b.y0, t),
        lerp(a.x1, b.x1, t),
        lerp(a.y1, b.y1, t),
    )
}

/// A shared-element transition: a widget's bounds morph from `from` to `to`.
/// Drive it by time ([`bounds_at`](SharedElement::bounds_at)) for a route
/// transition, or by a gesture fraction
/// ([`bounds_at_fraction`](SharedElement::bounds_at_fraction)) for an
/// interactive, interruptible drag.
#[derive(Clone, Copy, Debug)]
pub struct SharedElement {
    /// Start bounds.
    pub from: Rect,
    /// End bounds.
    pub to: Rect,
    /// Duration (ms) for the time-driven path.
    pub duration_ms: f64,
    /// Easing curve.
    pub easing: Easing,
}

impl SharedElement {
    /// Bounds at `now_ms` since the transition started.
    pub fn bounds_at(&self, now_ms: f64) -> Rect {
        let t = (now_ms / self.duration_ms).clamp(0.0, 1.0) as f32;
        lerp_rect(self.from, self.to, self.easing.apply(t))
    }

    /// Bounds at an explicit progress `fraction ∈ [0,1]` (gesture-driven).
    pub fn bounds_at_fraction(&self, fraction: f32) -> Rect {
        lerp_rect(
            self.from,
            self.to,
            self.easing.apply(fraction.clamp(0.0, 1.0)),
        )
    }

    /// Whether the time-driven transition has finished.
    pub fn settled(&self, now_ms: f64) -> bool {
        now_ms >= self.duration_ms
    }
}

/// One choreographed track: a scalar animating `from → to` after `delay_ms`.
#[derive(Clone, Copy, Debug)]
pub struct Track {
    /// Start delay (ms) — enables staggered choreography.
    pub delay_ms: f64,
    /// Duration (ms).
    pub duration_ms: f64,
    /// Start value.
    pub from: f32,
    /// End value.
    pub to: f32,
    /// Easing curve.
    pub easing: Easing,
}

impl Track {
    /// The track's value at `now_ms`.
    pub fn value_at(&self, now_ms: f64) -> f32 {
        let local = now_ms - self.delay_ms;
        if local <= 0.0 {
            return self.from;
        }
        if local >= self.duration_ms {
            return self.to;
        }
        let t = (local / self.duration_ms) as f32;
        self.from + (self.to - self.from) * self.easing.apply(t)
    }

    /// When this track finishes (ms).
    pub fn end_ms(&self) -> f64 {
        self.delay_ms + self.duration_ms
    }
}

/// A choreography of staggered tracks.
#[derive(Clone, Debug, Default)]
pub struct Timeline {
    /// Tracks to play in parallel (each with its own delay).
    pub tracks: Vec<Track>,
}

impl Timeline {
    /// Every track's value at `now_ms`.
    pub fn values(&self, now_ms: f64) -> Vec<f32> {
        self.tracks.iter().map(|t| t.value_at(now_ms)).collect()
    }

    /// Whether every track has finished (the whole sequence is idle).
    pub fn settled(&self, now_ms: f64) -> bool {
        self.tracks.iter().all(|t| now_ms >= t.end_ms())
    }

    /// Total duration of the choreography (ms).
    pub fn duration_ms(&self) -> f64 {
        self.tracks.iter().map(Track::end_ms).fold(0.0, f64::max)
    }
}

/// A critically-ish damped physics spring stepping 0 → 1 (re-exported helper).
pub fn spring(elapsed_secs: f32, stiffness: f32, damping: f32) -> f32 {
    spring_progress(elapsed_secs, stiffness, damping)
}
