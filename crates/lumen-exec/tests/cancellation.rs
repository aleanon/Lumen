//! Cancellation, run against every native spawner.
//!
//! The plan called for "parameterize the TC1 battery over the spawner". That
//! battery (`lumen-widgets/tests/data_layer.rs`) is built on `ManualSpawner`
//! and its `run_pending()` — deterministic stepping that a real runtime has no
//! equivalent of, so it cannot be handed a tokio spawner unchanged. What it can
//! be is *re-expressed*: the same scenarios, observed through a shared counter
//! and a bounded wait, so the answer for each spawner is comparable.
//!
//! The result is not a formality — the spawners genuinely differ, and the
//! difference is the point:
//!
//! | | starts a queued job? | stops a RUNNING one? |
//! |---|---|---|
//! | `ThreadPoolSpawner` | prevented by a flag | **no** |
//! | `TokioSpawner` | prevented | **yes**, at the next await |
//! | `SmolSpawner` | prevented | **yes**, at the next await |
//!
//! So this file asserts the weaker property for all of them and the stronger
//! one only where it holds, rather than testing the lowest common denominator
//! and learning nothing.
#![cfg(not(target_arch = "wasm32"))]

use lumen_core::tasks::{Spawner, ThreadPoolSpawner};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn wait_for(counter: &AtomicUsize, n: usize, limit: Duration) -> bool {
    let t0 = Instant::now();
    while t0.elapsed() < limit {
        if counter.load(Ordering::SeqCst) >= n {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

/// Every spawner runs what it is given.
fn runs_work(sp: &dyn Spawner, name: &str) {
    let n = Arc::new(AtomicUsize::new(0));
    let c = n.clone();
    sp.spawn(Box::pin(async move {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    assert!(
        wait_for(&n, 1, Duration::from_secs(5)),
        "{name}: spawned future never ran"
    );

    let n2 = Arc::new(AtomicUsize::new(0));
    let c2 = n2.clone();
    sp.spawn_blocking(Box::new(move || {
        c2.fetch_add(1, Ordering::SeqCst);
    }));
    assert!(
        wait_for(&n2, 1, Duration::from_secs(5)),
        "{name}: blocking job never ran"
    );
}

/// Aborting before the work starts prevents it. True of every spawner, and the
/// only cancellation guarantee the thread pool can make.
fn abort_before_start_prevents_it(sp: &dyn Spawner, name: &str) {
    let n = Arc::new(AtomicUsize::new(0));
    let c = n.clone();
    let h = sp.spawn(Box::pin(async move {
        // Long enough that the abort below lands first on any machine.
        std::thread::sleep(Duration::from_millis(400));
        c.fetch_add(1, Ordering::SeqCst);
    }));
    h.abort();
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(
        n.load(Ordering::SeqCst),
        0,
        "{name}: an aborted task ran anyway"
    );
}

#[cfg(any(feature = "tokio", feature = "smol"))]
/// A real runtime stops a task that is already running, at its next await.
/// The thread pool cannot, which is why it is not asked to.
fn abort_stops_a_running_task(sp: &dyn Spawner, name: &str) {
    let ticks = Arc::new(AtomicUsize::new(0));
    let t = ticks.clone();
    let h = sp.spawn(Box::pin(async move {
        for _ in 0..100 {
            t.fetch_add(1, Ordering::SeqCst);
            // An await point is where cancellation can land.
            futures_lite_sleep(Duration::from_millis(10)).await;
        }
    }));
    assert!(
        wait_for(&ticks, 2, Duration::from_secs(5)),
        "{name}: the task never got going, so this proves nothing about abort"
    );
    h.abort();
    std::thread::sleep(Duration::from_millis(120));
    let after = ticks.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(250));
    assert_eq!(
        ticks.load(Ordering::SeqCst),
        after,
        "{name}: a running task kept ticking after abort"
    );
    assert!(
        after < 100,
        "{name}: the task finished before the abort — timing too tight to prove anything"
    );
}

#[cfg(any(feature = "tokio", feature = "smol"))]
/// A runtime-agnostic sleep, so the shared body above does not bake in tokio.
async fn futures_lite_sleep(d: Duration) {
    // A yield-and-spin rather than a reactor timer: the point of the shared
    // body is to have an await point, not to test timers (that is `reactor.rs`).
    let end = Instant::now() + d;
    while Instant::now() < end {
        YieldOnce(false).await;
    }
}

#[cfg(any(feature = "tokio", feature = "smol"))]
struct YieldOnce(bool);
#[cfg(any(feature = "tokio", feature = "smol"))]
impl std::future::Future for YieldOnce {
    type Output = ();
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        if self.0 {
            std::task::Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    }
}

#[test]
fn thread_pool_runs_work_and_cancels_before_start() {
    let sp = ThreadPoolSpawner::default();
    runs_work(&sp, "thread-pool");
    abort_before_start_prevents_it(&sp, "thread-pool");
}

#[cfg(feature = "tokio")]
#[test]
fn tokio_runs_work_and_cancels_both_ways() {
    let sp = lumen_exec::TokioSpawner::multi_thread().expect("runtime");
    runs_work(&sp, "tokio");
    abort_before_start_prevents_it(&sp, "tokio");
    abort_stops_a_running_task(&sp, "tokio");
}

#[cfg(feature = "smol")]
#[test]
fn smol_runs_work_and_cancels_both_ways() {
    let sp = lumen_exec::SmolSpawner::new();
    runs_work(&sp, "smol");
    abort_before_start_prevents_it(&sp, "smol");
    abort_stops_a_running_task(&sp, "smol");
}
