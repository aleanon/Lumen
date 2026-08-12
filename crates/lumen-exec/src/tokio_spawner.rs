//! [`TokioSpawner`] — run Lumen's background work on a tokio runtime.

use lumen_core::tasks::{BlockingJob, BoxFuture, Spawner, TaskHandle};
use std::sync::Arc;
use tokio::runtime::{Handle, Runtime};

/// A [`Spawner`] backed by tokio.
///
/// Two ways to build one, because the two situations really are different:
///
/// * [`from_handle`](Self::from_handle) — the app already has a runtime. This
///   is the expected case and the cheapest: the adapter borrows and owns
///   nothing.
/// * [`multi_thread`](Self::multi_thread) / [`current_thread`](Self::current_thread)
///   — the adapter builds and owns one.
///
/// # What this buys over the thread pool
///
/// Futures are *driven*, not blocked on: `spawn` hands the future to tokio and
/// returns, so concurrency is bounded by tokio rather than by a thread count,
/// and reactor-dependent futures (timers, sockets) work at all.
///
/// Cancellation becomes real. [`TaskHandle::abort`] calls
/// `JoinHandle::abort`, which stops the task at its next await point instead of
/// waiting for a cooperative flag to be noticed.
///
/// # Two hazards worth knowing before you own a runtime
///
/// * **Dropping a `Runtime` from inside async context panics**, and dropping it
///   at all blocks until its tasks finish. An owned-runtime `TokioSpawner`
///   dropped on the UI thread during teardown can therefore hang. The owned
///   constructors shut down in the background on drop
///   (`Runtime::shutdown_background`) to avoid exactly that; a handle-borrowing
///   `TokioSpawner` has nothing to drop and cannot hit it.
/// * **`spawn_blocking` still cannot interrupt a running closure.** That is
///   tokio's limitation, not this adapter's: `abort` on a blocking job prevents
///   it starting, and nothing can stop it once it has. Long CPU work should
///   poll [`Sink::is_cancelled`](lumen_core::tasks::Sink::is_cancelled) the same
///   way it does on the thread pool.
pub struct TokioSpawner {
    handle: Handle,
    /// Kept alive only when we built the runtime ourselves. `None` means the
    /// caller owns it and outlives us.
    owned: Option<Arc<OwnedRuntime>>,
}

/// A runtime we created, shut down without blocking when it goes.
struct OwnedRuntime(Option<Runtime>);

impl Drop for OwnedRuntime {
    fn drop(&mut self) {
        if let Some(rt) = self.0.take() {
            // NOT a plain drop: that blocks until every task finishes, and this
            // runs on the UI thread when an app tears down.
            rt.shutdown_background();
        }
    }
}

impl TokioSpawner {
    /// Use a runtime the app already owns. Nothing is created or dropped here;
    /// the caller must keep the runtime alive at least as long as the app.
    pub fn from_handle(handle: Handle) -> TokioSpawner {
        TokioSpawner {
            handle,
            owned: None,
        }
    }

    /// Build and own a multi-threaded runtime.
    pub fn multi_thread() -> std::io::Result<TokioSpawner> {
        Self::owning(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?,
        )
    }

    /// Build and own a single-threaded runtime — enough for I/O-bound work, and
    /// one thread instead of one per core.
    pub fn current_thread() -> std::io::Result<TokioSpawner> {
        Self::owning(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?,
        )
    }

    fn owning(rt: Runtime) -> std::io::Result<TokioSpawner> {
        let handle = rt.handle().clone();
        Ok(TokioSpawner {
            handle,
            owned: Some(Arc::new(OwnedRuntime(Some(rt)))),
        })
    }

    /// The underlying handle, for app code that wants to spawn on the same
    /// runtime the UI uses.
    pub fn handle(&self) -> &Handle {
        &self.handle
    }
}

impl Clone for TokioSpawner {
    fn clone(&self) -> TokioSpawner {
        TokioSpawner {
            handle: self.handle.clone(),
            owned: self.owned.clone(),
        }
    }
}

/// A tokio task that can actually be stopped.
struct TokioHandle(tokio::task::JoinHandle<()>);

impl TaskHandle for TokioHandle {
    fn abort(&self) {
        self.0.abort();
    }
}

impl Spawner for TokioSpawner {
    fn spawn(&self, fut: BoxFuture) -> Box<dyn TaskHandle> {
        Box::new(TokioHandle(self.handle.spawn(fut)))
    }

    fn spawn_blocking(&self, f: BlockingJob) -> Box<dyn TaskHandle> {
        Box::new(TokioHandle(self.handle.spawn_blocking(f)))
    }
}
