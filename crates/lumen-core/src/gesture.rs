//! Touch gesture recognition (T3.5). A [`GestureRecognizer`] turns a stream of
//! timestamped touch points into high-level [`GestureEvent`]s (tap, double-tap,
//! long-press, pan, pinch). It is platform-independent and fully testable
//! headlessly: the mobile shells feed it OS touch events; tests synthesize the
//! same stream.

use crate::events::{GestureEvent, GestureKind};
use kurbo::Point;

/// A press held at least this long (ms) without moving is a long-press.
pub const LONG_PRESS_MS: f64 = 500.0;
/// A press shorter than this (ms) that does not move is a tap.
pub const TAP_MAX_MS: f64 = 300.0;
/// Movement under this many logical px does not count as a drag.
pub const TAP_SLOP_PX: f64 = 10.0;
/// Two taps within this window (ms) at the same spot form a double-tap.
pub const DOUBLE_TAP_MS: f64 = 300.0;

/// Lifecycle phase of a touch point.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TouchPhase {
    /// Finger down.
    Begin,
    /// Finger moved.
    Move,
    /// Finger lifted.
    End,
    /// Touch cancelled by the system.
    Cancel,
}

#[derive(Clone, Copy)]
struct Touch {
    id: u64,
    start: Point,
    last: Point,
    last_ms: f64,
    start_ms: f64,
    moved: bool,
}

/// Recognizes gestures from a stream of touch points keyed by pointer id.
#[derive(Default)]
pub struct GestureRecognizer {
    touches: Vec<Touch>,
    pinch_start_dist: Option<f64>,
    last_scale: f64,
    long_press_fired: bool,
    last_tap_ms: f64,
    last_tap_pos: Point,
}

impl GestureRecognizer {
    /// A new, idle recognizer.
    pub fn new() -> GestureRecognizer {
        GestureRecognizer::default()
    }

    fn centroid(&self) -> Point {
        if self.touches.is_empty() {
            return Point::ZERO;
        }
        let (mut x, mut y) = (0.0, 0.0);
        for t in &self.touches {
            x += t.last.x;
            y += t.last.y;
        }
        let n = self.touches.len() as f64;
        Point::new(x / n, y / n)
    }

    fn find(&mut self, id: u64) -> Option<usize> {
        self.touches.iter().position(|t| t.id == id)
    }

    /// Feed one touch point. Returns any gestures recognized by this event.
    pub fn feed(&mut self, ms: f64, phase: TouchPhase, id: u64, pos: Point) -> Vec<GestureEvent> {
        match phase {
            TouchPhase::Begin => self.begin(ms, id, pos),
            TouchPhase::Move => self.moved(ms, id, pos),
            TouchPhase::End => self.end(ms, id, pos),
            TouchPhase::Cancel => {
                if let Some(i) = self.find(id) {
                    self.touches.remove(i);
                }
                self.reset_multi();
                Vec::new()
            }
        }
    }

    /// Advance time without new input; emits a long-press once its threshold is
    /// crossed for a single stationary touch.
    pub fn tick(&mut self, ms: f64) -> Vec<GestureEvent> {
        if self.touches.len() == 1 && !self.long_press_fired {
            let t = self.touches[0];
            if !t.moved && ms - t.start_ms >= LONG_PRESS_MS {
                self.long_press_fired = true;
                return vec![GestureEvent {
                    kind: GestureKind::LongPress,
                    pos: t.last,
                    pointers: 1,
                }];
            }
        }
        Vec::new()
    }

    fn begin(&mut self, ms: f64, id: u64, pos: Point) -> Vec<GestureEvent> {
        self.touches.push(Touch {
            id,
            start: pos,
            last: pos,
            last_ms: ms,
            start_ms: ms,
            moved: false,
        });
        self.long_press_fired = false;
        if self.touches.len() == 2 {
            self.pinch_start_dist = Some(dist(self.touches[0].last, self.touches[1].last));
            self.last_scale = 1.0;
        }
        Vec::new()
    }

    fn moved(&mut self, ms: f64, id: u64, pos: Point) -> Vec<GestureEvent> {
        let Some(i) = self.find(id) else {
            return Vec::new();
        };
        let prev = self.touches[i].last;
        let prev_ms = self.touches[i].last_ms;
        let dt = ((ms - prev_ms) / 1000.0).max(1e-3);
        self.touches[i].last = pos;
        self.touches[i].last_ms = ms;
        if (pos - self.touches[i].start).hypot() > TAP_SLOP_PX {
            self.touches[i].moved = true;
        }

        // Two pointers -> pinch.
        if self.touches.len() == 2 {
            if let Some(start) = self.pinch_start_dist {
                let scale = dist(self.touches[0].last, self.touches[1].last) / start;
                let velocity = (scale - self.last_scale) / dt;
                self.last_scale = scale;
                return vec![GestureEvent {
                    kind: GestureKind::Pinch { scale, velocity },
                    pos: self.centroid(),
                    pointers: 2,
                }];
            }
        }

        // Single pointer that has moved -> pan.
        if self.touches.len() == 1 && self.touches[i].moved {
            let delta = pos - prev;
            return vec![GestureEvent {
                kind: GestureKind::Pan {
                    delta,
                    velocity: delta / dt,
                },
                pos,
                pointers: 1,
            }];
        }
        Vec::new()
    }

    fn end(&mut self, ms: f64, id: u64, pos: Point) -> Vec<GestureEvent> {
        let Some(i) = self.find(id) else {
            return Vec::new();
        };
        let t = self.touches.remove(i);
        self.reset_multi();

        // A quick, stationary single touch is a tap (or double-tap).
        let was_single = self.touches.is_empty();
        if was_single && !t.moved && !self.long_press_fired && ms - t.start_ms <= TAP_MAX_MS {
            let is_double = ms - self.last_tap_ms <= DOUBLE_TAP_MS
                && (pos - self.last_tap_pos).hypot() <= TAP_SLOP_PX
                && self.last_tap_ms > 0.0;
            if is_double {
                self.last_tap_ms = 0.0;
                return vec![GestureEvent {
                    kind: GestureKind::DoubleTap,
                    pos,
                    pointers: 1,
                }];
            }
            self.last_tap_ms = ms;
            self.last_tap_pos = pos;
            return vec![GestureEvent {
                kind: GestureKind::Tap,
                pos,
                pointers: 1,
            }];
        }
        Vec::new()
    }

    fn reset_multi(&mut self) {
        if self.touches.len() < 2 {
            self.pinch_start_dist = None;
        }
    }
}

fn dist(a: Point, b: Point) -> f64 {
    (a - b).hypot()
}
