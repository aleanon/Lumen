//! `Component` — the unit of rebuild.
//!
//! The trait is a thin layer over `cx.scope_with_deps`, so these tests are
//! about the *contract* it publishes rather than the memo machinery underneath
//! (which `scope_deps.rs` covers): build runs only when it should, and the
//! things a component is promised to keep across a rebuild are kept.

use kurbo::Size;
use lumen_core::state::Signal;
// `hash_of` is no longer imported: after S2 no test computes a deps hash by
// hand. That absence is the phase's result.
use lumen_widgets::{widgets, App, BuildCx, Component, Element, SIGNALS_ONLY};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A component whose output is a function of captured plain data.
///
/// `builds` is a test probe, not render input, and `&AtomicUsize` is not `Hash`
/// anyway — so this hashes only the fields that affect rendering. That is the
/// override path S2 leaves open for handlers and `f64`s, and writing it by hand
/// is a deliberate statement that the omitted field does not affect output.
struct Titled {
    title: String,
    n: i64,
    builds: &'static AtomicUsize,
}

impl std::hash::Hash for Titled {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.title.hash(h);
        self.n.hash(h);
    }
}

impl Component for Titled {
    fn build(&self, _cx: &mut BuildCx) -> Element {
        self.builds.fetch_add(1, Ordering::Relaxed);
        widgets::text(format!("{} {}", self.title, self.n))
    }
}

static A: AtomicUsize = AtomicUsize::new(0);

#[test]
fn build_is_skipped_while_deps_are_unchanged() {
    A.store(0, Ordering::Relaxed);
    let mut h = App::new(|cx: &mut BuildCx| {
        // The signal exists only to force a rebuild of the ROOT; the
        // component's own deps never change, so it must not re-build.
        let tick: Signal<i64> = cx.signal("tick", || 0);
        let _ = tick.get(cx.runtime());
        cx.component(
            "t",
            Titled {
                title: "hello".into(),
                n: 7,
                builds: &A,
            },
        )
    })
    .run_headless(Size::new(200.0, 80.0));
    h.pump();
    let after_first = A.load(Ordering::Relaxed);
    assert_eq!(after_first, 1, "built once on the first frame");

    let tick: Signal<i64> = h.runtime().signal("tick", || 0);
    for i in 1..5 {
        tick.set(h.runtime(), i);
        h.pump();
    }
    assert_eq!(
        A.load(Ordering::Relaxed),
        1,
        "the root rebuilt four more times; the component's deps never changed, \
         so its build must not have run again"
    );
    assert!(h.semantics_json().to_string().contains("hello 7"));
    h.assert_view_coherent();
}

static B: AtomicUsize = AtomicUsize::new(0);

#[test]
fn changed_deps_rebuild_it() {
    B.store(0, Ordering::Relaxed);
    let mut h = App::new(|cx: &mut BuildCx| {
        let n: Signal<i64> = cx.signal("n", || 0);
        let cur = n.get(cx.runtime());
        cx.component(
            "t",
            Titled {
                title: "count".into(),
                n: cur,
                builds: &B,
            },
        )
    })
    .run_headless(Size::new(200.0, 80.0));
    h.pump();
    assert_eq!(B.load(Ordering::Relaxed), 1);

    let n: Signal<i64> = h.runtime().signal("n", || 0);
    n.set(h.runtime(), 42);
    h.pump();
    assert_eq!(B.load(Ordering::Relaxed), 2, "deps moved, so it rebuilt");
    assert!(
        h.semantics_json().to_string().contains("count 42"),
        "and the new value is on screen"
    );
    h.assert_view_coherent();
}

/// S1 × S2: a component that captures nothing and reads a `#[derive(Reactive)]`
/// field inside `build`. It declares no data dependency at all — the read
/// tracker supplies it — and the field is root-scoped, so the test can move it.
#[allow(dead_code)]
#[derive(lumen_widgets::Reactive, Default)]
#[reactive(crate = "lumen_core")]
struct Model {
    v: i64,
}

struct FromSignal(&'static AtomicUsize);

impl std::hash::Hash for FromSignal {
    fn hash<H: std::hash::Hasher>(&self, _h: &mut H) {
        // Captures nothing that affects rendering.
    }
}

impl Component for FromSignal {
    // `deps` overridden to say "no captured data" explicitly. Hashing the unit
    // would give the same constant; SIGNALS_ONLY states the intent.
    fn deps(&self) -> u64 {
        SIGNALS_ONLY
    }
    fn build(&self, cx: &mut BuildCx) -> Element {
        self.0.fetch_add(1, Ordering::Relaxed);
        widgets::text(format!("v={}", Model::v_signal(cx).get(cx.runtime())))
    }
}

static C: AtomicUsize = AtomicUsize::new(0);

#[test]
fn signals_read_inside_build_are_tracked_without_being_declared() {
    C.store(0, Ordering::Relaxed);
    let mut h = App::new(|cx: &mut BuildCx| cx.component("s", FromSignal(&C)))
        .run_headless(Size::new(200.0, 80.0));
    h.pump();
    assert_eq!(C.load(Ordering::Relaxed), 1);
    assert!(h.semantics_json().to_string().contains("v=0"));

    // The original version of this test stopped here — it asserted that a
    // signals-only component *renders*, never that its read actually
    // invalidates it. That is the whole claim, and it was untested: a build
    // that ignored reads entirely would have passed. Move the signal.
    //
    Model::v_signal(h.runtime()).set(h.runtime(), 5);
    h.pump();
    assert_eq!(
        C.load(Ordering::Relaxed),
        2,
        "a signal read inside build must re-run it, with no deps declared"
    );
    assert!(h.semantics_json().to_string().contains("v=5"));
    h.assert_view_coherent();
}

/// Two components of the same type are distinct because their keys are — the
/// contract `component`'s `key` parameter exists to enforce.
static D: AtomicUsize = AtomicUsize::new(0);

#[test]
fn siblings_are_distinguished_by_key() {
    D.store(0, Ordering::Relaxed);
    let mut h = App::new(|cx: &mut BuildCx| {
        widgets::column(vec![
            cx.component(
                "left",
                Titled {
                    title: "L".into(),
                    n: 1,
                    builds: &D,
                },
            ),
            cx.component(
                "right",
                Titled {
                    title: "R".into(),
                    n: 2,
                    builds: &D,
                },
            ),
        ])
    })
    .run_headless(Size::new(200.0, 120.0));
    h.pump();
    assert_eq!(D.load(Ordering::Relaxed), 2, "both built");
    let doc = h.semantics_json().to_string();
    assert!(doc.contains("L 1"), "left rendered: {doc}");
    assert!(
        doc.contains("R 2"),
        "right rendered — a shared key would have collapsed them into one"
    );
    h.assert_view_coherent();
}

// ---------------------------------------------------------------------------
// S2: `deps` derived from the fields
// ---------------------------------------------------------------------------

/// The shape S2 exists for: `#[derive(Hash)]`, **no `deps` at all**.
///
/// Under C1 this component was impossible to write — `deps` was required, and
/// the failure mode it guarded against was an author omitting a captured field
/// and getting silently frozen content. The default hashes every field, so the
/// omission cannot happen.
#[derive(std::hash::Hash)]
struct Derived {
    n: i64,
    tag: &'static str,
}

// A THREAD-LOCAL, not a static. The harness runs tests in parallel threads, and
// a shared static counter is reset and incremented by every test at once — it
// has produced a spurious failure in this repo's tests four separate times,
// each time looking exactly like a real memoization bug. A thread-local is
// isolated per test by construction.
thread_local! {
    static BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
fn builds() -> usize {
    BUILDS.with(|b| b.get())
}
fn reset_builds() {
    BUILDS.with(|b| b.set(0));
}

impl Component for Derived {
    // No `deps`. That is the point.
    fn build(&self, _cx: &mut BuildCx) -> Element {
        BUILDS.with(|b| b.set(b.get() + 1));
        widgets::text(format!("{}:{}", self.tag, self.n))
    }
}

#[test]
fn deps_default_to_hashing_the_component() {
    reset_builds();
    let mut h = App::new(|cx: &mut BuildCx| {
        let n: Signal<i64> = cx.signal("n", || 0);
        let cur = n.get(cx.runtime());
        // A rebuild-forcing signal the component does NOT capture, so a
        // memo-hit is attributable to the derived deps rather than to nothing
        // having changed.
        let _tick: Signal<i64> = cx.signal("tick", || 0);
        let _ = _tick.get(cx.runtime());
        cx.component("d", Derived { n: cur, tag: "row" })
    })
    .run_headless(Size::new(200.0, 80.0));
    h.pump();
    assert_eq!(builds(), 1);
    assert!(h.semantics_json().to_string().contains("row:0"));

    // Root rebuilds, component's captured fields unchanged ⇒ memo hit.
    let tick: Signal<i64> = h.runtime().signal("tick", || 0);
    for i in 1..4 {
        tick.set(h.runtime(), i);
        h.pump();
    }
    assert_eq!(
        builds(),
        1,
        "three root rebuilds with the component's fields unchanged: the derived \
         deps must memo-hit"
    );

    // A captured field moves ⇒ rebuild, with no `deps` written by hand.
    let n: Signal<i64> = h.runtime().signal("n", || 0);
    n.set(h.runtime(), 9);
    h.pump();
    assert_eq!(
        builds(),
        2,
        "the captured field changed, so the derived deps changed"
    );
    assert!(h.semantics_json().to_string().contains("row:9"));
    h.assert_view_coherent();
}

/// Every field participates — including one a hand-written `deps` would be
/// most likely to forget, because it is not the one being displayed.
#[test]
fn a_field_that_is_not_rendered_still_counts() {
    reset_builds();
    let mut h = App::new(|cx: &mut BuildCx| {
        let t: Signal<i64> = cx.signal("t", || 0);
        let cur = t.get(cx.runtime());
        // `tag` varies; only `n` is shown first. A hand-written deps hashing
        // just the displayed value would memo-hit and freeze.
        cx.component(
            "d",
            Derived {
                n: 0,
                tag: if cur == 0 { "a" } else { "b" },
            },
        )
    })
    .run_headless(Size::new(200.0, 80.0));
    h.pump();
    assert!(h.semantics_json().to_string().contains("a:0"));

    let t: Signal<i64> = h.runtime().signal("t", || 0);
    t.set(h.runtime(), 1);
    h.pump();
    assert!(
        h.semantics_json().to_string().contains("b:0"),
        "the changed field is part of the derived deps even though the other \
         field is unchanged"
    );
    h.assert_view_coherent();
}
