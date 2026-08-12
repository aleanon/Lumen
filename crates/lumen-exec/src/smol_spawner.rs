//! [`SmolSpawner`] — run Lumen's background work on smol's executor.

use lumen_core::tasks::{BlockingJob, BoxFuture, Spawner, TaskHandle};
use std::sync::Mutex;

/// A [`Spawner`] backed by smol.
///
/// # Who drives it
///
/// `smol::spawn` uses **`async-global-executor`**, which starts its own worker
/// threads on first use and needs nothing from the app — no runtime to build,
/// hold or drop, and no equivalent of tokio's "dropping a runtime in async
/// context panics" hazard. Stated explicitly rather than left ambient, because
/// "who runs my futures" is the first question anyone has about an executor
/// adapter. Thread count follows `SMOL_THREADS` (default: one per core).
///
/// # Cancellation
///
/// Cleaner than tokio's, and much cleaner than the thread pool's: `smol::spawn`
/// returns a `Task` that **cancels when dropped**, so
/// [`TaskHandle::abort`] simply drops it. The task stops at its next await
/// point.
///
/// `spawn_blocking` goes to `blocking::unblock`, whose thread pool has the same
/// limitation every such pool does: a closure already running cannot be
/// interrupted. Long CPU work should poll
/// [`Sink::is_cancelled`](lumen_core::tasks::Sink::is_cancelled).
#[derive(Default, Clone, Copy)]
pub struct SmolSpawner;

impl SmolSpawner {
    /// A spawner on smol's global executor.
    pub fn new() -> SmolSpawner {
        SmolSpawner
    }
}

/// Holds the `Task`, and the two ways of letting go of it mean opposite things.
///
/// smol's `Task` **cancels when dropped**, which does not match `Spawner`'s
/// contract: a caller that spawns and discards the handle expects the work to
/// run, the way it does on tokio and the thread pool. Caught by
/// `tests/cancellation.rs::runs_work`, which does exactly that and saw the
/// future never execute.
///
/// So: `abort()` drops the `Task` (cancel), and `Drop` **detaches** it (let it
/// finish). Both take it out of the slot, and whichever happens first wins.
///
/// `Mutex` because `TaskHandle::abort` takes `&self`; it is uncontended.
struct SmolHandle(Mutex<Option<smol::Task<()>>>);

impl TaskHandle for SmolHandle {
    fn abort(&self) {
        // Dropping the `Task` IS the cancel. `.cancel()` is equivalent but
        // returns a future that must be awaited to observe completion, and
        // `abort` is synchronous by contract.
        if let Ok(mut slot) = self.0.lock() {
            slot.take();
        }
    }
}

impl Drop for SmolHandle {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.0.lock() {
            if let Some(task) = slot.take() {
                // Letting go of the handle is not a cancellation.
                task.detach();
            }
        }
    }
}

impl Spawner for SmolSpawner {
    fn spawn(&self, fut: BoxFuture) -> Box<dyn TaskHandle> {
        Box::new(SmolHandle(Mutex::new(Some(smol::spawn(fut)))))
    }

    fn spawn_blocking(&self, f: BlockingJob) -> Box<dyn TaskHandle> {
        // `unblock` moves the closure to a dedicated blocking pool and gives
        // back a future; spawning that keeps the same cancel-on-drop shape as
        // `spawn`, so both halves of the trait behave alike.
        Box::new(SmolHandle(Mutex::new(Some(smol::spawn(
            blocking::unblock(f),
        )))))
    }
}
