//! ADR-021 H2: hash identity must not cost the *names* the framework reports.
//!
//! Identity is a hash now, but two features stayed name-keyed on purpose:
//! **snapshot restore** (ADR-011 — `StateSnapshot` is field-tagged JSON keyed by
//! a readable name) and **agent observability** (ADR-009 — `scope_deps` says
//! *why* a subtree updates). The name is built once, on the cold intern path;
//! these tests pin that it is still the *right* name — including through scopes
//! and for typed (non-string) keys.
#![cfg(feature = "snapshot")]

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App, BuildCx, Element};

/// A typed key — the shape ADR-021 exists to make cheap.
#[derive(Hash, Debug, Clone, Copy)]
enum Field {
    Total,
    Row(u32),
}

fn rows_view(cx: &mut BuildCx) -> Element {
    let total: Signal<i64> = cx.signal(Field::Total, || 0i64);
    let mut kids = vec![widgets::text(format!("total={}", total.get(cx.runtime()))).id("total")];
    for i in 0..3u32 {
        kids.push(cx.scope(Field::Row(i), move |cx| {
            // Scope-local state, keyed by a plain name *inside* the scope.
            let hits: Signal<i64> = cx.signal("hits", || 0i64);
            widgets::text(format!("row{i}={}", hits.get(cx.runtime()))).id(format!("row-{i}"))
        }));
    }
    widgets::column(kids)
}

/// A snapshot taken with typed + scoped keys must restore into a fresh instance.
/// This is the round-trip the readable name exists for: if a scoped signal's
/// name were wrong (or missing), its value would silently reset to `init`.
#[test]
fn snapshot_round_trips_through_scoped_and_typed_keys() {
    let mut a = App::new(rows_view).run_headless(Size::new(300.0, 240.0));
    a.pump();

    // Write via the root-level typed key...
    let total: Signal<i64> = a.runtime().signal(Field::Total, || 0i64);
    total.set(a.runtime(), 42);
    // ...and via a scope-local key, addressed the way the build folded it.
    let row1 = lumen_core::identity::ScopePath::root().child(Field::Row(1));
    let hits: Signal<i64> = a.runtime().signal_at(
        lumen_core::identity::fold_id(row1.hash(), lumen_core::identity::hash_id("hits")),
        row1.hash(),
        || "unused-existing".to_string(),
        || 0i64,
    );
    hits.set(a.runtime(), 7);
    a.pump();
    assert!(a.semantics_json().to_string().contains("row1=7"));

    let snap = a.snapshot();

    let (mut b, diags) = App::new(rows_view).run_headless_restored(Size::new(300.0, 240.0), snap);
    assert!(
        diags.is_empty(),
        "clean restore raises no diagnostics: {diags:?}"
    );
    b.pump();
    let sem = b.semantics_json().to_string();
    assert!(
        sem.contains("total=42"),
        "typed root key lost its value across a snapshot: {sem}"
    );
    assert!(
        sem.contains("row1=7"),
        "scope-local state lost its value across a snapshot: {sem}"
    );
    b.assert_view_coherent();
}

/// Snapshot keys are what a human (and a restore) reads. A typed key must
/// serialize under a readable name, and a `&str` key must keep the *exact*
/// spelling it had before ADR-021 — `Debug` would otherwise quote it (`"n"`),
/// orphaning every existing snapshot.
#[test]
fn snapshot_keys_are_readable_and_unquoted() {
    let mut a = App::new(rows_view).run_headless(Size::new(300.0, 240.0));
    a.pump();
    let json = a.runtime().snapshot().0;
    let obj = json.as_object().expect("snapshot is a JSON object");
    let keys: Vec<&str> = obj.keys().map(String::as_str).collect();

    assert!(
        keys.contains(&"Total"),
        "typed key should serialize readably, got {keys:?}"
    );
    assert!(
        keys.contains(&"Row(1)/hits"),
        "scope-local key should carry its scope path, got {keys:?}"
    );
    assert!(
        !keys.iter().any(|k| k.starts_with('"')),
        "no snapshot key may be Debug-quoted, got {keys:?}"
    );
}

/// The agent reports *why* a subtree updates by dep name (ADR-009, `getDeps`).
/// Those names come from the same cold-path builder, so a scoped dep must still
/// be attributed with its full path rather than a bare local name.
#[test]
fn scope_deps_report_readable_scoped_names() {
    let mut a = App::new(rows_view).run_headless(Size::new(300.0, 240.0));
    a.pump();

    let doc = a.semantics_json().to_string();
    assert!(
        doc.contains("Row(1)/hits"),
        "a scope's dep should be reported by its readable scoped name: {doc}"
    );
}
