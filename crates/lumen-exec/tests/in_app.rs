//! The adapters drive a real app's `cx.task`, not just bare futures.
//!
//! Standalone spawner tests prove the trait impl; they do not prove the result
//! reaches the UI. That round trip — future → `Sink` → deferred op → waker →
//! `pump` → signal → rebuild — is the thing an app actually depends on, and it
//! is the part a spawner can break by, say, running the future on a thread the
//! deferred queue never drains.
#![cfg(all(feature = "tokio", not(target_arch = "wasm32")))]

use lumen_core::geometry::Size;
use lumen_core::state::Signal;
use lumen_exec::TokioSpawner;
use lumen_widgets::{widgets, App, BuildCx, Element};
use std::time::{Duration, Instant};

/// An app whose scope spawns a task that writes a signal after a tokio timer —
/// so the value can only arrive if a reactor drove it.
fn build(cx: &mut BuildCx) -> Element {
    let n: Signal<i32> = cx.signal("n", || 0);
    cx.task("load", (), move |_deps, sink| async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        sink.set(n, 42);
    });
    widgets::column(vec![widgets::text(format!("{}", n.get(cx.runtime())))]).id("root")
}

#[test]
fn a_tokio_task_result_reaches_the_ui() {
    let mut h = App::new(build)
        .with_executor(TokioSpawner::multi_thread().expect("runtime"))
        .run_headless(Size::new(200.0, 80.0));
    h.pump();

    let n: Signal<i32> = h.runtime().signal("n", || 0);
    assert_eq!(n.get(h.runtime()), 0, "starts unloaded");

    // Pump until the deferred result lands. Real time, not virtual: the whole
    // point is that a real runtime is doing the waiting.
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(5) && n.get(h.runtime()) == 0 {
        std::thread::sleep(Duration::from_millis(5));
        h.pump();
    }
    assert_eq!(
        n.get(h.runtime()),
        42,
        "the task's result never reached the app"
    );
}
