//! The test that justifies this crate: a reactor-dependent future.
//!
//! `ThreadPoolSpawner::spawn` does `block_on(fut)` on a pool thread. There is
//! no tokio runtime in that thread's context, so `tokio::time::sleep` does not
//! run slowly — it **panics**. That is the whole argument for these adapters,
//! made executable rather than asserted in a doc comment.
#![cfg(all(feature = "tokio", not(target_arch = "wasm32")))]

use lumen_core::tasks::{Spawner, ThreadPoolSpawner};
use lumen_exec::TokioSpawner;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn wait_until(flag: &AtomicBool, limit: Duration) -> bool {
    let t0 = Instant::now();
    while t0.elapsed() < limit {
        if flag.load(Ordering::SeqCst) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

/// Under tokio the timer resolves and the future finishes.
#[test]
fn a_tokio_timer_completes_under_the_tokio_spawner() {
    let sp = TokioSpawner::multi_thread().expect("runtime");
    let done = Arc::new(AtomicBool::new(false));
    let d = done.clone();
    sp.spawn(Box::pin(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        d.store(true, Ordering::SeqCst);
    }));
    assert!(
        wait_until(&done, Duration::from_secs(5)),
        "the timer never fired"
    );
}

/// Under the thread pool the same future never completes — there is no reactor
/// to drive the timer, and tokio panics inside the worker.
///
/// Asserted as "did not finish" rather than "panicked", because the panic
/// happens on a pool thread this test does not own. The observable difference
/// is what matters: identical code, one spawner completes it and the other
/// cannot.
#[test]
fn the_same_timer_never_completes_on_the_thread_pool() {
    let sp = ThreadPoolSpawner::default();
    let done = Arc::new(AtomicBool::new(false));
    let d = done.clone();
    sp.spawn(Box::pin(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        d.store(true, Ordering::SeqCst);
    }));
    assert!(
        !wait_until(&done, Duration::from_millis(600)),
        "a tokio timer completed without a tokio runtime — if this ever passes, \
         the thread-pool spawner grew a reactor and this crate's premise needs \
         re-checking"
    );
}
