//! Edge-triggering primitives for agent-facing observability (O0.2).
//!
//! # Why these exist
//!
//! The diagnostic log ring holds **1000 entries** and evicts the oldest
//! ([`Runtime::log`](crate::Runtime::log)). At 120 fps a single unconditional
//! per-frame line flushes the entire ring in eight seconds, taking every
//! startup fact and every real finding with it. So every observability site in
//! the framework must report a *transition* rather than a *state*.
//!
//! That constraint is easy to state and easy to get subtly wrong per site,
//! which is what these three types are for. They are deliberately small: the
//! codebase already latches with a bare `Cell<bool>` in places
//! (`atlas_overflow` in the GPU backend), and nothing here is cleverer than
//! that — only named, tested, and reusable.
//!
//! # Single-threaded by design
//!
//! Everything here uses `Cell`/`RefCell` and no atomics. The runtime is
//! `Rc`-backed and lives on the UI thread; adding `Send`/`Sync` bounds would
//! buy nothing and would cost the wasm target, which has no threads at all.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// A one-way edge detector: reports the moment a condition becomes true, and
/// stays quiet while it *remains* true.
///
/// For "we entered a degraded regime" — the GPU fell back to the CPU, the
/// swapchain went stale, the text cache started thrashing. The condition may
/// hold for thousands of frames; the interesting event is the crossing.
///
/// ```
/// use lumen_core::observe::Latch;
/// let degraded = Latch::new();
/// assert!(degraded.set(true));   // crossed — report it
/// assert!(!degraded.set(true));  // still true — say nothing
/// assert!(!degraded.set(false)); // recovered — not an event for this type
/// assert!(degraded.set(true));   // crossed again — report again
/// ```
#[derive(Debug, Default)]
pub struct Latch {
    on: Cell<bool>,
}

impl Latch {
    /// A latch that has not fired.
    pub fn new() -> Latch {
        Latch {
            on: Cell::new(false),
        }
    }

    /// Record the condition's current value; returns `true` **only** on a
    /// `false → true` crossing.
    pub fn set(&self, on: bool) -> bool {
        let crossed = on && !self.on.get();
        self.on.set(on);
        crossed
    }

    /// Whether the condition currently holds.
    pub fn is_set(&self) -> bool {
        self.on.get()
    }

    /// Forget the current state, so the next `true` reports as a fresh
    /// crossing. For a full reset (a rebuilt app, a reloaded stylesheet).
    pub fn reset(&self) {
        self.on.set(false);
    }
}

/// Per-pass presence diff: reports keys present now that were absent last pass.
///
/// This is the shape the ambient audit needs, and it is **not** a
/// "have I ever seen this" set. That distinction is the whole point:
///
/// * A monotonic seen-set needs an invalidation policy, and every candidate
///   policy in this codebase is wrong. Clearing on `rebuild_fresh()` misses
///   ordinary state-driven rebuilds entirely — `pump` calls `rebuild()`, and
///   `rebuild_fresh` only runs from the coherence oracle — so the dominant
///   dev-session loop (break it, fix it, break it again by toggling a signal)
///   would report the first break and then go silent forever.
/// * A presence diff needs no policy at all. A finding that goes away and
///   comes back is a `false → true` crossing per key, which is exactly what a
///   regression is.
///
/// ```
/// use lumen_core::observe::FrameDiff;
/// let mut d = FrameDiff::new();
/// assert_eq!(d.newly_present(["a", "b"]), vec!["a", "b"]);
/// assert!(d.newly_present(["a", "b"]).is_empty()); // unchanged — quiet
/// assert_eq!(d.newly_present(["a", "b", "c"]), vec!["c"]);
/// assert!(d.newly_present(["a"]).is_empty());      // b, c went away
/// assert_eq!(d.newly_present(["a", "b"]), vec!["b"]); // b came back — report
/// ```
#[derive(Debug)]
pub struct FrameDiff<K> {
    prev: HashSet<K>,
}

impl<K> Default for FrameDiff<K> {
    fn default() -> Self {
        FrameDiff {
            prev: HashSet::new(),
        }
    }
}

impl<K: Eq + Hash + Clone> FrameDiff<K> {
    /// An empty diff — the first pass reports everything it is given.
    pub fn new() -> FrameDiff<K> {
        FrameDiff::default()
    }

    /// Swap in this pass's key set and return the keys that were absent from
    /// the previous one, **in the order given** so callers keep whatever
    /// ordering their source imposed.
    pub fn newly_present<I: IntoIterator<Item = K>>(&mut self, current: I) -> Vec<K> {
        let current: Vec<K> = current.into_iter().collect();
        let fresh: Vec<K> = current
            .iter()
            .filter(|k| !self.prev.contains(*k))
            .cloned()
            .collect();
        self.prev = current.into_iter().collect();
        fresh
    }

    /// Forget the previous pass entirely, so the next one reports everything
    /// again. For a hard reset where continuity is meaningless.
    pub fn clear(&mut self) {
        self.prev.clear();
    }

    /// How many keys the previous pass carried.
    pub fn len(&self) -> usize {
        self.prev.len()
    }

    /// Whether the previous pass was empty.
    pub fn is_empty(&self) -> bool {
        self.prev.is_empty()
    }
}

/// Rate limiter for anomalies that are legitimately recurrent: allow the first
/// `first_n` occurrences, then one in every `every`.
///
/// For events that are noise in isolation and signal in bulk — a dropped
/// present during a resize drag is routine, a hundred of them means the window
/// has stopped updating.
///
/// Keys are **explicit and caller-chosen**, not `#[track_caller]` locations: a
/// `Location` is not a stable dedup identity (it moves when the file is
/// edited), and an explicit key lets one site throttle several distinct
/// conditions.
///
/// ```
/// use lumen_core::observe::Throttle;
/// let t = Throttle::new(2, 10);
/// assert!(t.allow("present-skipped"));  // 1st
/// assert!(t.allow("present-skipped"));  // 2nd
/// assert!(!t.allow("present-skipped")); // 3rd — suppressed
/// // ...and a different key has its own budget.
/// assert!(t.allow("other"));
/// ```
#[derive(Debug)]
pub struct Throttle {
    counts: RefCell<HashMap<&'static str, u64>>,
    first_n: u64,
    every: u64,
}

impl Throttle {
    /// Allow the first `first_n` occurrences of each key, then every `every`th.
    /// `every` is clamped to at least 1.
    pub fn new(first_n: u64, every: u64) -> Throttle {
        Throttle {
            counts: RefCell::new(HashMap::new()),
            first_n,
            every: every.max(1),
        }
    }

    /// Record an occurrence of `key`; returns whether it should be reported.
    pub fn allow(&self, key: &'static str) -> bool {
        let mut counts = self.counts.borrow_mut();
        let n = counts.entry(key).or_insert(0);
        *n += 1;
        let n = *n;
        n <= self.first_n || (n - self.first_n).is_multiple_of(self.every)
    }

    /// Total occurrences recorded for `key`, reported or not. Worth including
    /// in the message when a throttled site finally does report — "the 400th
    /// of these" is the part that makes it actionable.
    pub fn count(&self, key: &str) -> u64 {
        self.counts.borrow().get(key).copied().unwrap_or(0)
    }

    /// Forget all counts.
    pub fn reset(&self) {
        self.counts.borrow_mut().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latch_reports_only_the_crossing() {
        let l = Latch::new();
        assert!(l.set(true), "first true is the event");
        for _ in 0..10_000 {
            assert!(!l.set(true), "a held condition must stay quiet");
        }
        assert!(l.is_set());
        l.set(false);
        assert!(l.set(true), "re-entering the regime is a new event");
    }

    #[test]
    fn frame_diff_needs_no_invalidation_policy() {
        let mut d = FrameDiff::new();
        assert_eq!(d.newly_present(["w0103:a"]), vec!["w0103:a"]);
        // Held: silent, however long it holds.
        for _ in 0..10_000 {
            assert!(d.newly_present(["w0103:a"]).is_empty());
        }
        // Fixed.
        assert!(d.newly_present::<[&str; 0]>([]).is_empty());
        // Re-broken by ordinary state — the case a monotonic seen-set with a
        // `rebuild_fresh`-keyed clear would have missed forever.
        assert_eq!(d.newly_present(["w0103:a"]), vec!["w0103:a"]);
    }

    #[test]
    fn frame_diff_distinguishes_nodes_not_just_codes() {
        let mut d = FrameDiff::new();
        assert_eq!(d.newly_present(["w0103:a"]), vec!["w0103:a"]);
        assert_eq!(
            d.newly_present(["w0103:a", "w0103:b"]),
            vec!["w0103:b"],
            "a second node with the same code is a distinct finding"
        );
    }

    #[test]
    fn frame_diff_preserves_input_order() {
        let mut d = FrameDiff::new();
        assert_eq!(d.newly_present(["c", "a", "b"]), vec!["c", "a", "b"]);
    }

    #[test]
    fn throttle_allows_a_burst_then_thins_out() {
        let t = Throttle::new(3, 5);
        let allowed = (0..23).filter(|_| t.allow("k")).count();
        // 3 in the burst, then #8, #13, #18, #23.
        assert_eq!(allowed, 7, "burst plus every 5th");
        assert_eq!(t.count("k"), 23, "the total is still recorded");
    }

    #[test]
    fn throttle_budgets_are_per_key() {
        let t = Throttle::new(1, 1000);
        assert!(t.allow("a"));
        assert!(!t.allow("a"));
        assert!(t.allow("b"), "a separate condition has its own budget");
    }
}
