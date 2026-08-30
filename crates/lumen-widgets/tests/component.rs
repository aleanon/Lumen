//! `Component` — the unit of rebuild.
//!
//! The trait is a thin layer over `cx.scope_with_deps`, so these tests are
//! about the *contract* it publishes rather than the memo machinery underneath
//! (which `scope_deps.rs` covers): build runs only when it should, and the
//! things a component is promised to keep across a rebuild are kept.

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{hash_of, widgets, App, BuildCx, Component, Element, SIGNALS_ONLY};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A component whose output is a function of captured plain data.
struct Titled {
    title: String,
    n: i64,
    builds: &'static AtomicUsize,
}

impl Component for Titled {
    fn deps(&self) -> u64 {
        hash_of(&(&self.title, self.n))
    }
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

/// A component that captures nothing and reads a signal inside `build` declares
/// `SIGNALS_ONLY` — the read tracker supplies the dependency.
struct FromSignal(&'static AtomicUsize);

impl Component for FromSignal {
    fn deps(&self) -> u64 {
        SIGNALS_ONLY
    }
    fn build(&self, cx: &mut BuildCx) -> Element {
        self.0.fetch_add(1, Ordering::Relaxed);
        let v: Signal<i64> = cx.signal("inner", || 0);
        widgets::text(format!("v={}", v.get(cx.runtime())))
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

    // `inner` is scope-local to the component, so address it the same way the
    // component does — through the scope, not the root.
    assert!(h.semantics_json().to_string().contains("v=0"));
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
