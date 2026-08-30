//! S1 — `#[derive(Reactive)]`: field-path keys into the existing store.
//!
//! The value of this phase is **correctness**, not speed: an integer key was
//! already allocation-free (ADR-021), so a field path is equally fast, not
//! faster. What it buys is that identity stops being an author-written string.
//! These tests pin the properties that makes true, including the two bugs found
//! in this repo's own benchmarks that it renders unrepresentable.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Reactive};

// The fields are never *read* from the struct: in S1 the struct is a key
// namespace and the values live in the store. That is the phase's shape, not an
// oversight — S3 is where the field becomes the slot.
#[allow(dead_code)]
#[derive(Reactive, Default)]
struct Counter {
    count: i64,
    label: String,
}

/// A second struct with the *same field names* — the collision case an
/// author-written string key gets wrong the first time two screens both want
/// a `count`.
#[allow(dead_code)]
#[derive(Reactive, Default)]
struct Other {
    count: i64,
}

#[test]
fn a_field_reads_and_writes_through_its_own_signal() {
    let mut h = App::new(|cx: &mut BuildCx| {
        widgets::text(format!("n={}", Counter::count(cx).get(cx.runtime())))
    })
    .run_headless(Size::new(200.0, 80.0));
    h.pump();
    assert!(h.semantics_json().to_string().contains("n=0"), "default");

    Counter::count(h.runtime()).set(h.runtime(), 41);
    h.pump();
    assert!(
        h.semantics_json().to_string().contains("n=41"),
        "a write through the generated accessor reaches the view"
    );
    h.assert_view_coherent();
}

/// Same field name, different struct ⇒ different signal. The key is the *path*,
/// not the name.
#[test]
fn identically_named_fields_of_different_structs_do_not_collide() {
    let mut h = App::new(|cx: &mut BuildCx| {
        widgets::column(vec![
            widgets::text(format!("c={}", Counter::count(cx).get(cx.runtime()))),
            widgets::text(format!("o={}", Other::count(cx).get(cx.runtime()))),
        ])
    })
    .run_headless(Size::new(200.0, 120.0));
    h.pump();

    Counter::count(h.runtime()).set(h.runtime(), 5);
    h.pump();
    let doc = h.semantics_json().to_string();
    assert!(doc.contains("c=5"), "the written one moved: {doc}");
    assert!(
        doc.contains("o=0"),
        "the identically-named field of the other struct did NOT — a bare \
         \"count\" key would have moved both"
    );
    h.assert_view_coherent();
}

/// Fields of one struct are independent of each other.
#[test]
fn fields_of_one_struct_are_independent() {
    let mut h = App::new(|cx: &mut BuildCx| {
        widgets::text(format!(
            "{}|{}",
            Counter::count(cx).get(cx.runtime()),
            Counter::label(cx).get(cx.runtime())
        ))
    })
    .run_headless(Size::new(240.0, 80.0));
    h.pump();
    Counter::label(h.runtime()).set(h.runtime(), "hi".to_string());
    h.pump();
    assert!(h.semantics_json().to_string().contains("0|hi"));
    h.assert_view_coherent();
}

/// **The bug this phase exists to make unrepresentable.**
///
/// `BuildCx::signal` namespaces by the enclosing scope, so reading a signal by
/// the same key inside and outside a scope addresses *different* slots — which
/// is correct for view-local state and silently wrong for app state. It cost
/// this repo a benchmark that reported a fast number for a frame that never
/// updated. A generated accessor uses `Runtime::signal`, rooted at `ROOT_ID`,
/// so it reads the same field from anywhere.
#[test]
fn a_field_reads_the_same_slot_inside_and_outside_a_scope() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let outer = Counter::count(cx).get(cx.runtime());
        let inner = cx.scope("s", |cx2| {
            widgets::text(format!("inner={}", Counter::count(cx2).get(cx2.runtime())))
        });
        widgets::column(vec![widgets::text(format!("outer={outer}")), inner])
    })
    .run_headless(Size::new(240.0, 120.0));
    h.pump();
    Counter::count(h.runtime()).set(h.runtime(), 9);
    h.pump();
    h.pump(); // a moved signal can lag one frame through a restyle

    let doc = h.semantics_json().to_string();
    assert!(doc.contains("outer=9"), "read outside the scope: {doc}");
    assert!(
        doc.contains("inner=9"),
        "read INSIDE a scope must address the same field — with \
         BuildCx::signal this would still read 0"
    );
    h.assert_view_coherent();
}

/// D2's requirement: a reload that drops a field must still report it. The
/// generated key is a stable string in the snapshot, so `finish_restore`
/// reports it as W0002 exactly as a hand-keyed signal would — plain
/// `#[serde(default)]` would drop it silently.
#[test]
fn a_dropped_field_is_still_reported_on_restore() {
    let mut h = App::new(|cx: &mut BuildCx| {
        widgets::text(format!("{}", Counter::count(cx).get(cx.runtime())))
    })
    .run_headless(Size::new(200.0, 80.0));
    h.pump();
    Counter::count(h.runtime()).set(h.runtime(), 3);
    h.pump();

    let snap = h.runtime().snapshot();
    let json = serde_json::to_string(&snap.0).expect("snapshot serializes");
    assert!(
        json.contains("Counter") && json.contains("count"),
        "the field path is the snapshot key, so it is readable and stable: {json}"
    );

    // Restore into an app that no longer reads `Counter::count` — the field was
    // removed from the struct between reloads.
    let mut h2 = App::new(|_cx: &mut BuildCx| widgets::text("no counter here"))
        .run_headless(Size::new(200.0, 80.0));
    h2.runtime().load_pending(snap);
    h2.pump();
    let diags = h2.runtime().finish_restore();
    assert!(
        diags.iter().any(|d| d.code == lumen_core::codes::W0002),
        "a field present in the snapshot and absent from the new build must be \
         reported, not silently dropped; got {diags:?}"
    );
}
