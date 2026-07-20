//! The async / background-work layer (the data layer).
//!
//! The reactive [`Runtime`] is single-threaded (`Rc<RefCell<…>>`, **not** `Send`),
//! so background work can never mutate the store directly. Instead it holds a
//! [`Sink`] and pushes a [`DeferredOp`] onto a channel; the runtime drains that
//! channel on the UI thread at the top of the next turn ([`Runtime::drain_deferred`]),
//! applying each op. This keeps `pump()` a pure function of (state, queued
//! events, clock) — the invariant that makes goldens, agent replay, and
//! snapshot/restore sound.
//!
//! Work is run by a [`Spawner`] the host provides; the runtime is generic over
//! it (`E: Spawner`, defaulting to [`InlineSpawner`]). A `Box<dyn Spawner>` is
//! itself a `Spawner` (blanket impl), so a consumer who wants a backend chosen at
//! runtime opts in by instantiating with `E = Box<dyn Spawner>`.

use crate::state::{Runtime, Signal, State};
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

/// M.5 (ADR-M2): `Send` where threads exist, nothing on wasm — the
/// platform-conditional bound that lets ONE generic surface fit tokio
/// handles, the thread pool, and browser `spawn_local`-style executors
/// (wasm futures — `fetch` — are `!Send`, and there are no threads to
/// cross anyway).
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + ?Sized> MaybeSend for T {}
/// wasm: no threads, no bound.
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSend for T {}

/// A pending state mutation produced off-thread, applied on the UI thread.
/// (Trait objects can only carry auto-trait bounds, so the platform split is
/// on the alias, not `MaybeSend`.)
#[cfg(not(target_arch = "wasm32"))]
pub type DeferredOp = Box<dyn FnOnce(&Runtime) + Send>;
/// wasm: single-threaded — no `Send`.
#[cfg(target_arch = "wasm32")]
pub type DeferredOp = Box<dyn FnOnce(&Runtime)>;

/// A boxed blocking job for [`Spawner::spawn_blocking`]. `Send` on native
/// (it crosses to a pool thread); wasm runs it inline on the only thread.
#[cfg(not(target_arch = "wasm32"))]
pub type BlockingJob = Box<dyn FnOnce() + Send>;
/// wasm: inline, no `Send`.
#[cfg(target_arch = "wasm32")]
pub type BlockingJob = Box<dyn FnOnce()>;

/// A boxed future — the unit of async work a [`Spawner`] runs. `Send` on
/// native; wasm futures are `!Send`.
#[cfg(not(target_arch = "wasm32"))]
pub type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send>>;
/// wasm: `!Send` futures welcome.
#[cfg(target_arch = "wasm32")]
pub type BoxFuture = Pin<Box<dyn Future<Output = ()>>>;

/// Wakes the host event loop after a deferred op is queued, so a frame gets
/// scheduled. Set by the shell; absent in headless/tests (where the executor is
/// inline or manually driven, and the next `pump` drains the queue).
pub type WakeFn = Arc<dyn Fn() + Send + Sync>;

/// A channel from background work back to the runtime. Lives on the [`Runtime`];
/// the `Sender` (in each [`Sink`]) is `Send` and crosses threads, the `Receiver`
/// stays on the UI thread.
pub(crate) struct DeferredChannel {
    tx: Sender<DeferredOp>,
    rx: RefCell<Receiver<DeferredOp>>,
}

impl DeferredChannel {
    pub(crate) fn new() -> DeferredChannel {
        let (tx, rx) = channel();
        DeferredChannel {
            tx,
            rx: RefCell::new(rx),
        }
    }
}

/// Handed to background work; its only job is to push a result back and wake the
/// loop. Carries **no** executor type — just a channel sender + an optional waker
/// — so task closures and user fetchers never name the executor `E`.
#[derive(Clone)]
pub struct Sink {
    tx: Sender<DeferredOp>,
    waker: Option<WakeFn>,
}

impl Sink {
    /// Enqueue an arbitrary mutation applied on the UI thread next turn (the
    /// flexible, **non-replayable** escape hatch).
    pub fn mutate(&self, f: impl FnOnce(&Runtime) + MaybeSend + 'static) {
        if self.tx.send(Box::new(f)).is_ok() {
            if let Some(w) = &self.waker {
                w();
            }
        }
    }

    /// Set `sig` to `v` (applied next turn). Value-based ⇒ recordable/replayable.
    pub fn set<T: State + MaybeSend>(&self, sig: Signal<T>, v: T) {
        self.mutate(move |rt| sig.set(rt, v));
    }

    /// Update `sig` in place (applied next turn).
    pub fn update<T: State + MaybeSend>(
        &self,
        sig: Signal<T>,
        f: impl FnOnce(&mut T) + MaybeSend + 'static,
    ) {
        self.mutate(move |rt| sig.update(rt, f));
    }
}

impl Runtime {
    /// Mint a [`Sink`] bound to this runtime's deferred-op channel (no waker —
    /// the next manual `drain_deferred`/`pump` applies its ops).
    pub fn make_sink(&self) -> Sink {
        self.make_sink_with(None)
    }

    /// Mint a [`Sink`] with a host waker (the shell wires an event-loop wake so
    /// background results schedule a frame).
    pub fn make_sink_with(&self, waker: Option<WakeFn>) -> Sink {
        Sink {
            tx: self.deferred().tx.clone(),
            waker,
        }
    }

    /// Apply every queued [`DeferredOp`] on the UI thread, returning the count.
    /// Called at the top of `pump`. Ops are collected first, then applied, so an
    /// op may itself enqueue more (drained next turn).
    pub fn drain_deferred(&self) -> usize {
        let ops: Vec<DeferredOp> = {
            let ch = self.deferred();
            let rx = ch.rx.borrow();
            rx.try_iter().collect()
        };
        let n = ops.len();
        for op in ops {
            op(self);
        }
        n
    }
}

/// Runs background work. Implemented by the host; the runtime is generic over it.
/// Object-safe (boxed args) so `Box<dyn Spawner>` is a valid backend.
pub trait Spawner {
    /// Run a future to completion off the UI thread.
    fn spawn(&self, fut: BoxFuture);
    /// Run a blocking closure off the UI thread (CPU-bound work).
    fn spawn_blocking(&self, f: BlockingJob);
}

/// A boxed spawner is itself a spawner — the dynamic-dispatch opt-in.
impl<S: Spawner + ?Sized> Spawner for Box<S> {
    fn spawn(&self, fut: BoxFuture) {
        (**self).spawn(fut)
    }
    fn spawn_blocking(&self, f: BlockingJob) {
        (**self).spawn_blocking(f)
    }
}

/// The deterministic default: runs blocking work inline and block-on's futures to
/// completion on the calling thread. No threads ⇒ goldens/tests stay bit-stable
/// and resources resolve "immediately" in virtual time. (A truly-suspending
/// future would block the UI thread — use a thread-pool / async executor for
/// real I/O; this is for tests and ready/compute work.)
#[derive(Default, Clone, Copy)]
pub struct InlineSpawner;

impl Spawner for InlineSpawner {
    fn spawn(&self, fut: BoxFuture) {
        block_on(fut);
    }
    fn spawn_blocking(&self, f: BlockingJob) {
        f();
    }
}

/// Records spawned work instead of running it; [`ManualSpawner::run_pending`]
/// runs it. Lets a test step through intermediate (loading) states
/// deterministically. Cheap-clone (shared), so a test can keep a handle after
/// the spawner is moved into the runtime.
#[derive(Default, Clone)]
pub struct ManualSpawner {
    pending: Rc<RefCell<Vec<Job>>>,
}

enum Job {
    Future(BoxFuture),
    Blocking(BlockingJob),
}

impl ManualSpawner {
    /// A fresh manual spawner.
    pub fn new() -> ManualSpawner {
        ManualSpawner::default()
    }

    /// Number of jobs recorded but not yet run.
    pub fn pending(&self) -> usize {
        self.pending.borrow().len()
    }

    /// Run all recorded jobs (futures block-on to completion). Their results land
    /// on the deferred-op channel; call `pump`/`drain_deferred` to apply them.
    pub fn run_pending(&self) -> usize {
        let jobs: Vec<Job> = std::mem::take(&mut *self.pending.borrow_mut());
        let n = jobs.len();
        for job in jobs {
            match job {
                Job::Future(fut) => block_on(fut),
                Job::Blocking(f) => f(),
            }
        }
        n
    }
}

impl Spawner for ManualSpawner {
    fn spawn(&self, fut: BoxFuture) {
        self.pending.borrow_mut().push(Job::Future(fut));
    }
    fn spawn_blocking(&self, f: BlockingJob) {
        self.pending.borrow_mut().push(Job::Blocking(f));
    }
}

/// A real executor backed by a small pool of OS threads (native only — wasm has
/// no threads). `spawn_blocking` queues the closure; `spawn` queues
/// `block_on(fut)`. The default for desktop/Android shells.
#[cfg(not(target_arch = "wasm32"))]
pub struct ThreadPoolSpawner {
    tx: std::sync::mpsc::Sender<Box<dyn FnOnce() + Send>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ThreadPoolSpawner {
    /// A pool with `workers` threads (clamped to ≥1).
    pub fn new(workers: usize) -> ThreadPoolSpawner {
        let (tx, rx) = std::sync::mpsc::channel::<Box<dyn FnOnce() + Send>>();
        let rx = Arc::new(std::sync::Mutex::new(rx));
        for _ in 0..workers.max(1) {
            let rx = Arc::clone(&rx);
            std::thread::spawn(move || loop {
                // Hold the lock only across recv; run the job unlocked so workers
                // run jobs concurrently.
                let job = {
                    let guard = rx.lock().expect("pool rx");
                    guard.recv()
                };
                match job {
                    Ok(j) => j(),
                    Err(_) => break, // sender dropped → shut down
                }
            });
        }
        ThreadPoolSpawner { tx }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for ThreadPoolSpawner {
    fn default() -> ThreadPoolSpawner {
        let n = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        ThreadPoolSpawner::new(n)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Spawner for ThreadPoolSpawner {
    fn spawn(&self, fut: BoxFuture) {
        let _ = self.tx.send(Box::new(move || block_on(fut)));
    }
    fn spawn_blocking(&self, f: BlockingJob) {
        let _ = self.tx.send(f);
    }
}

/// A minimal `block_on`: poll the future, parking the thread until woken. Used by
/// the inline/manual executors (std has no `block_on`).
fn block_on(mut fut: BoxFuture) {
    struct Unparker(std::thread::Thread);
    impl Wake for Unparker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unparker(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(()) => return,
            Poll::Pending => std::thread::park(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sink_set_is_applied_on_drain() {
        let rt = Runtime::new();
        let sig = rt.signal("n", || 0i32);
        let sink = rt.make_sink();
        // Simulate a background task pushing a result.
        sink.set(sig, 42);
        assert_eq!(sig.get(&rt), 0, "not applied until drained");
        let n = rt.drain_deferred();
        assert_eq!(n, 1);
        assert_eq!(sig.get(&rt), 42, "applied on drain");
    }

    #[test]
    fn inline_spawner_runs_blocking_and_futures() {
        let rt = Runtime::new();
        let sig = rt.signal("s", || 0i32);
        let ex = InlineSpawner;
        let sink = rt.make_sink();
        let s2 = sink.clone();
        ex.spawn_blocking(Box::new(move || s2.set(sig, 7)));
        ex.spawn(Box::pin(async move { sink.set(sig, 9) }));
        rt.drain_deferred();
        assert_eq!(sig.get(&rt), 9, "both ran inline; last write wins");
    }

    #[test]
    fn thread_pool_runs_work_off_thread() {
        use std::sync::mpsc::channel;
        let pool = ThreadPoolSpawner::new(2);
        let (tx, rx) = channel();
        pool.spawn_blocking(Box::new(move || tx.send(7).unwrap()));
        assert_eq!(rx.recv().unwrap(), 7, "blocking job ran on the pool");
        let (tx2, rx2) = channel();
        pool.spawn(Box::pin(async move { tx2.send(9).unwrap() }));
        assert_eq!(rx2.recv().unwrap(), 9, "future job ran on the pool");
    }

    #[test]
    fn manual_spawner_defers_until_run() {
        let rt = Runtime::new();
        let sig = rt.signal("m", || 0i32);
        let ex = ManualSpawner::new();
        let sink = rt.make_sink();
        ex.spawn_blocking(Box::new(move || sink.set(sig, 5)));
        assert_eq!(ex.pending(), 1);
        rt.drain_deferred();
        assert_eq!(sig.get(&rt), 0, "job not run yet");
        ex.run_pending();
        rt.drain_deferred();
        assert_eq!(sig.get(&rt), 5, "run + drain applies it");
    }
}

// --- M.5 (ADR-M2): the wasm executor ----------------------------------------

/// wasm: a dependency-free single-thread executor. `spawn` queues the future;
/// the host's RAF tick drives it via [`pump_wasm_tasks`] (completion lands
/// through [`Sink`] like every other executor — the framework never drives
/// foreign wakers beyond its own ready flag). `spawn_blocking` runs inline:
/// there is no other thread to run it on.
#[cfg(target_arch = "wasm32")]
#[derive(Default, Clone, Copy)]
pub struct WasmSpawner;

#[cfg(target_arch = "wasm32")]
thread_local! {
    static WASM_TASKS: RefCell<Vec<BoxFuture>> = const { RefCell::new(Vec::new()) };
}

#[cfg(target_arch = "wasm32")]
impl Spawner for WasmSpawner {
    fn spawn(&self, fut: BoxFuture) {
        WASM_TASKS.with(|q| q.borrow_mut().push(fut));
    }
    fn spawn_blocking(&self, f: BlockingJob) {
        f(); // single thread: inline (document in the skill; keep jobs small)
    }
}

/// Poll every queued wasm task once (RAF cadence). Returns whether any tasks
/// remain pending — the host keeps ticking while true.
#[cfg(target_arch = "wasm32")]
pub fn pump_wasm_tasks() -> bool {
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    fn raw() -> RawWaker {
        fn no(_: *const ()) {}
        fn cl(_: *const ()) -> RawWaker {
            raw()
        }
        RawWaker::new(std::ptr::null(), &RawWakerVTable::new(cl, no, no, no))
    }
    let waker = unsafe { Waker::from_raw(raw()) };
    let mut cx = Context::from_waker(&waker);
    WASM_TASKS.with(|q| {
        let mut tasks = q.take();
        tasks.retain_mut(|fut| fut.as_mut().poll(&mut cx) == Poll::Pending);
        let pending = !tasks.is_empty();
        q.borrow_mut().extend(tasks);
        pending
    })
}
