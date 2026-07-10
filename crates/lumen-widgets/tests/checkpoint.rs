//! W.4b (docs/plan-remediation-2026-07.md): the `Checkpoint` trait (02 §4,
//! ADR-011) — quiesce/serialize/restore/resume formalized over the snapshot
//! machinery, including **live** restore into a running instance (existing
//! signals adopt in place; the prior path only covered a fresh boot via
//! `run_headless_restored`).
#![cfg(feature = "snapshot")]

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{col, widgets, App, BuildCx, Checkpoint, Element};

fn counter_view(cx: &mut BuildCx) -> Element {
    let n: Signal<i64> = cx.signal("n", || 0i64);
    col![widgets::text(format!("n={}", n.get(cx.runtime()))).id("out")]
}

#[test]
fn restore_into_running_instance_adopts_in_place() {
    // Source instance: bump the counter, quiesce, capture.
    let mut a = App::new(counter_view).run_headless(Size::new(300.0, 200.0));
    a.pump();
    let n: Signal<i64> = a.runtime().signal("n", || 0i64);
    n.set(a.runtime(), 5);
    a.quiesce();
    let snap = a.serialize_state();

    // Target instance: already running with its own (different) state.
    let mut b = App::new(counter_view).run_headless(Size::new(300.0, 200.0));
    b.pump();
    assert!(b.semantics_json().to_string().contains("n=0"));

    let diags = b.restore_state(snap);
    assert!(
        diags.is_empty(),
        "clean restore raises no diagnostics: {diags:?}"
    );
    assert!(
        b.semantics_json().to_string().contains("n=5"),
        "live signal adopted the snapshot value in place"
    );
    b.resume();
    b.assert_view_coherent();
}

fn src_view(cx: &mut BuildCx) -> Element {
    cx.signal("n", || 1i64);
    cx.signal("gone", || 9i64);
    col![widgets::text("src").id("t")]
}

fn dst_view(cx: &mut BuildCx) -> Element {
    cx.signal("n", || 0i64);
    col![widgets::text("dst").id("t")]
}

#[test]
fn restore_drops_stale_keys_with_w0002() {
    // Source app stores a signal the target app no longer declares.
    let mut a = App::new(src_view).run_headless(Size::new(300.0, 200.0));
    a.quiesce();
    let snap = a.serialize_state();

    let mut b = App::new(dst_view).run_headless(Size::new(300.0, 200.0));
    b.pump();

    let diags = b.restore_state(snap);
    assert!(
        diags
            .iter()
            .any(|d| d.code == "W0002" && d.message.contains("gone")),
        "stale snapshot key reported as W0002 drop: {diags:?}"
    );
    let n: Signal<i64> = b.runtime().signal("n", || 0i64);
    assert_eq!(n.get(b.runtime()), 1, "surviving key restored");
}
