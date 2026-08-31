//! S1 — `#[derive(Reactive)]`: field-path keys into the existing store.
//!
//! The value of this phase is **correctness**, not speed: an integer key was
//! already allocation-free (ADR-021), so a field path is equally fast, not
//! faster. What it buys is that identity stops being an author-written string.
//! These tests pin the properties that makes true, including the two bugs found
//! in this repo's own benchmarks that it renders unrepresentable.

use kurbo::Size;
use lumen_widgets::{bind, widgets, App, BuildCx, Reactive};
use std::cell::Cell;
use std::rc::Rc;

// The fields are never *read* from the struct: in S1 the struct is a key
// namespace and the values live in the store. That is the phase's shape, not an
// oversight — S3 is where the field becomes the slot.
#[allow(dead_code)]
#[derive(Reactive, Default)]
#[reactive(crate = "lumen_core")]
struct Counter {
    count: i64,
    label: String,
}

/// A second struct with the *same field names* — the collision case an
/// author-written string key gets wrong the first time two screens both want
/// a `count`.
#[allow(dead_code)]
#[derive(Reactive, Default)]
#[reactive(crate = "lumen_core")]
struct Other {
    count: i64,
}

#[test]
fn a_field_reads_and_writes_through_its_own_signal() {
    let mut h = App::new(|cx: &mut BuildCx| {
        widgets::text(format!("n={}", Counter::count_signal(cx).get(cx.runtime())))
    })
    .run_headless(Size::new(200.0, 80.0));
    h.pump();
    assert!(h.semantics_json().to_string().contains("n=0"), "default");

    Counter::count_signal(h.runtime()).set(h.runtime(), 41);
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
            widgets::text(format!("c={}", Counter::count_signal(cx).get(cx.runtime()))),
            widgets::text(format!("o={}", Other::count_signal(cx).get(cx.runtime()))),
        ])
    })
    .run_headless(Size::new(200.0, 120.0));
    h.pump();

    Counter::count_signal(h.runtime()).set(h.runtime(), 5);
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
            Counter::count_signal(cx).get(cx.runtime()),
            Counter::label_signal(cx).get(cx.runtime())
        ))
    })
    .run_headless(Size::new(240.0, 80.0));
    h.pump();
    Counter::label_signal(h.runtime()).set(h.runtime(), "hi".to_string());
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
        let outer = Counter::count_signal(cx).get(cx.runtime());
        let inner = cx.scope("s", |cx2| {
            widgets::text(format!(
                "inner={}",
                Counter::count_signal(cx2).get(cx2.runtime())
            ))
        });
        widgets::column(vec![widgets::text(format!("outer={outer}")), inner])
    })
    .run_headless(Size::new(240.0, 120.0));
    h.pump();
    Counter::count_signal(h.runtime()).set(h.runtime(), 9);
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
        widgets::text(format!("{}", Counter::count_signal(cx).get(cx.runtime())))
    })
    .run_headless(Size::new(200.0, 80.0));
    h.pump();
    Counter::count_signal(h.runtime()).set(h.runtime(), 3);
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

// ---- MUT8: the instance-threaded state model ----

#[derive(Reactive)]
#[reactive(crate = "lumen_core")]
#[cfg_attr(feature = "snapshot", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "snapshot", serde(default))]
struct Dash {
    count: i64,
    title: String,
}

impl Default for Dash {
    fn default() -> Self {
        Dash {
            count: 0,
            title: "hello".into(),
        }
    }
}

#[test]
fn with_state_reads_are_field_refs_and_writes_are_field_grained() {
    // The view reads two fields through the derive's instance accessors; each
    // field is its own scope dep, so writing one re-runs one scope only.
    let count_runs = Rc::new(Cell::new(0u32));
    let title_runs = Rc::new(Cell::new(0u32));
    let (cr, tr) = (count_runs.clone(), title_runs.clone());
    let mut h = App::with_state(Dash::default(), move |cx: &mut BuildCx, s: &Dash| {
        let cr = cr.clone();
        let tr = tr.clone();
        let c = *s.count(cx);
        let t = s.title(cx).clone();
        let a = cx.scope_with_deps("c", c, move |_| {
            cr.set(cr.get() + 1);
            widgets::text(format!("count {c}"))
        });
        let b = cx.scope_with_deps("t", t.clone(), move |_| {
            tr.set(tr.get() + 1);
            widgets::text(format!("title {t}"))
        });
        widgets::column(vec![a, b])
    })
    .run_headless(Size::new(240.0, 120.0));

    assert_eq!((count_runs.get(), title_runs.get()), (1, 1));
    Dash::set_count(h.runtime(), 7);
    h.pump();
    assert_eq!(count_runs.get(), 2, "the written field's scope re-ran");
    assert_eq!(title_runs.get(), 1, "the other field's scope spliced");
    assert!(h.semantics_json().to_string().contains("count 7"));
    h.assert_view_coherent();
}

#[test]
fn with_state_binding_patches_through_get_accessor() {
    // A bind! closure has only a Runtime — the derive's get_* form records
    // the field read, so the reverse index routes the write to the binding
    // and the frame patches instead of rebuilding.
    let build_runs = Rc::new(Cell::new(0u32));
    let br = build_runs.clone();
    let mut h = App::with_state(Dash::default(), move |_cx: &mut BuildCx, _s: &Dash| {
        br.set(br.get() + 1);
        let mut root = widgets::column(vec![widgets::text(bind!(rt => {
            format!("n = {}", Dash::get_count(rt))
        }))]);
        root.style.width = lumen_layout::Dim::pct(1.0);
        root
    })
    .run_headless(Size::new(240.0, 120.0));

    assert_eq!(build_runs.get(), 1);
    Dash::update_count(h.runtime(), |c| *c += 41);
    h.pump();
    assert_eq!(
        build_runs.get(),
        1,
        "a bound field write patches, not rebuilds"
    );
    assert!(h.semantics_json().to_string().contains("n = 41"));
    h.assert_view_coherent();
}

#[cfg(feature = "snapshot")]
#[test]
fn with_state_reloads_by_serde_and_defaults_missing_fields() {
    // The user's iced recipe: serialize, swap code, deserialize with
    // #[serde(default)]. A field present in the snapshot survives; one the
    // snapshot lacks takes its default.
    let mut h = App::with_state(Dash::default(), |_cx: &mut BuildCx, s: &Dash| {
        widgets::column(vec![widgets::text(format!(
            "{} #{}",
            s.title.clone(),
            s.count
        ))])
    })
    .run_headless(Size::new(240.0, 120.0));
    Dash::set_count(h.runtime(), 9);
    Dash::set_title(h.runtime(), "saved".into());
    h.pump();
    let snap = h.runtime().snapshot();

    // "Swap code": a fresh app boots, stages the snapshot, adopts it live.
    let mut h2 = App::with_state(Dash::default(), |cx: &mut BuildCx, s: &Dash| {
        widgets::column(vec![widgets::text(format!(
            "{} #{}",
            s.title(cx).clone(),
            s.count(cx)
        ))])
    })
    .run_headless(Size::new(240.0, 120.0));
    h2.runtime().load_pending(snap);
    h2.runtime().adopt_pending_live();
    h2.pump();
    assert!(
        h2.semantics_json().to_string().contains("saved #9"),
        "the instance restored by serde"
    );
    h2.assert_view_coherent();
}
