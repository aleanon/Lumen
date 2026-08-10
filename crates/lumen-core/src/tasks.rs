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
//!
//! # Cancellation (TC1)
//!
//! Two independent mechanisms, because neither alone is sufficient:
//!
//! * A [`CancelToken`] rides on the [`Sink`]. It is the *correctness* half:
//!   once set, queued [`DeferredOp`]s are dropped **at apply time**, so a task
//!   that outlives the signals it writes can no longer reach them. Tasks can
//!   also poll it ([`Sink::is_cancelled`]) to bail out of a loop.
//! * A [`TaskHandle`], returned by [`Spawner::spawn`], is the *resource* half:
//!   it stops the work itself where the backend can (drop a queued job or an
//!   unpolled future). What that amounts to is deliberately uneven — see
//!   [`TaskHandle`].

use crate::state::{Runtime, Signal, State};
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// A one-way "stop" flag shared between the UI thread and the work it spawned —
/// the only piece of the cancellation machinery that crosses threads.
///
/// Set by the runtime when a task's owning scope dies or its deps are
/// superseded, and by an app-level abort handle on demand.
/// Once set it never clears: a cancelled task identity is finished, and a
/// re-declared one mints a fresh token.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A fresh, live token.
    pub fn new() -> CancelToken {
        CancelToken::default()
    }

    /// Signal cancellation. Idempotent.
    pub fn cancel(&self) {
        // `Relaxed` is sufficient: the flag publishes no other data, so there is
        // nothing for an acquire/release pair to order. The UI thread's own
        // apply-time check is program-ordered after its `cancel`, and a worker
        // polling `is_cancelled` only needs eventual visibility.
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether cancellation has been signalled.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// A backend's handle on spawned work, returned by [`Spawner::spawn`].
///
/// **`abort` is best-effort, and honestly uneven across backends** — it stops
/// work the backend still owns, and nothing else:
///
/// | Backend | What `abort` does |
/// |---|---|
/// | [`InlineSpawner`] | nothing — the work ran to completion during `spawn` |
/// | [`ManualSpawner`] | drops the job if `run_pending` has not reached it |
/// | [`ThreadPoolSpawner`] | drops the job if no worker has picked it up; a *running* job is unaffected (std cannot stop a thread) |
/// | `WasmSpawner` | drops the future — real cancellation |
/// | a tokio backend | `JoinHandle::abort()` — real cancellation |
///
/// This is why the [`CancelToken`] exists alongside it: the token is what makes
/// cancellation *correct* (no writes land after it), while `abort` is what makes
/// it *cheap* (the work stops burning a thread). Code that must stop promptly
/// once running has to poll [`Sink::is_cancelled`] itself.
pub trait TaskHandle {
    /// Stop the work if this backend still can.
    fn abort(&self);
}

/// The handle for work that cannot be stopped — already finished, or already
/// handed to a thread that owns it.
pub struct NoopHandle;

impl TaskHandle for NoopHandle {
    fn abort(&self) {}
}

/// A shared flag a spawner sets to skip a job its queue has not yet started.
/// Distinct from [`CancelToken`] on purpose: a [`Spawner`] receives an opaque
/// boxed job and never sees the app-level token, so it carries its own.
/// (Only [`ThreadPoolSpawner`] needs it — the other backends own their queue
/// and can drop the job itself.)
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Default)]
struct SkipFlag(Arc<AtomicBool>);

#[cfg(not(target_arch = "wasm32"))]
impl SkipFlag {
    fn skipped(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl TaskHandle for SkipFlag {
    fn abort(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

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
/// loop. Carries **no** executor type — just a channel sender, an optional waker
/// and a [`CancelToken`] — so task closures and user fetchers never name the
/// executor `E`.
#[derive(Clone)]
pub struct Sink {
    tx: Sender<DeferredOp>,
    waker: Option<WakeFn>,
    token: CancelToken,
}

impl Sink {
    /// Whether this task has been cancelled — its scope died, its deps were
    /// superseded, or an [`AbortHandle`](TaskHandle) fired.
    ///
    /// Every write through this sink is already a no-op once cancelled, so
    /// polling this is about not doing *work* (stop reading the socket, stop
    /// hashing) rather than about correctness. A long loop should check it each
    /// iteration; a one-shot fetch need not check it at all.
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// Enqueue an arbitrary mutation applied on the UI thread next turn (the
    /// flexible, **non-replayable** escape hatch). A cancelled sink drops it.
    pub fn mutate(&self, f: impl FnOnce(&Runtime) + MaybeSend + 'static) {
        // Cheap early-out: a task that has already been told to stop never
        // queues more work.
        if self.token.is_cancelled() {
            return;
        }
        // The check that actually matters is the one *inside* the op.
        // Cancellation can land between this send and the next `drain_deferred`,
        // and by then the signals this op targets may have been evicted with
        // their scope — where `Signal::update` panics rather than no-opping.
        let token = self.token.clone();
        let op: DeferredOp = Box::new(move |rt| {
            if !token.is_cancelled() {
                f(rt);
            }
        });
        if self.tx.send(op).is_ok() {
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
    /// background results schedule a frame), and a token that never fires.
    pub fn make_sink_with(&self, waker: Option<WakeFn>) -> Sink {
        self.make_sink_for(waker, CancelToken::new())
    }

    /// Mint a [`Sink`] bound to an existing [`CancelToken`] — the form the
    /// runtime uses when dispatching a task it intends to be able to cancel.
    pub fn make_sink_for(&self, waker: Option<WakeFn>, token: CancelToken) -> Sink {
        Sink {
            tx: self.deferred().tx.clone(),
            waker,
            token,
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
/// Object-safe (boxed args *and* boxed return) so `Box<dyn Spawner>` stays a
/// valid backend.
pub trait Spawner {
    /// Run a future to completion off the UI thread, returning a handle that can
    /// stop it if this backend is able to (see [`TaskHandle`]).
    fn spawn(&self, fut: BoxFuture) -> Box<dyn TaskHandle>;
    /// Run a blocking closure off the UI thread (CPU-bound work), returning a
    /// handle that can stop it if this backend is able to.
    fn spawn_blocking(&self, f: BlockingJob) -> Box<dyn TaskHandle>;
}

/// A boxed spawner is itself a spawner — the dynamic-dispatch opt-in.
impl<S: Spawner + ?Sized> Spawner for Box<S> {
    fn spawn(&self, fut: BoxFuture) -> Box<dyn TaskHandle> {
        (**self).spawn(fut)
    }
    fn spawn_blocking(&self, f: BlockingJob) -> Box<dyn TaskHandle> {
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
    // Both return `NoopHandle`: the work is already finished by the time the
    // caller could hold the handle, so there is nothing left to abort.
    fn spawn(&self, fut: BoxFuture) -> Box<dyn TaskHandle> {
        block_on(fut);
        Box::new(NoopHandle)
    }
    fn spawn_blocking(&self, f: BlockingJob) -> Box<dyn TaskHandle> {
        f();
        Box::new(NoopHandle)
    }
}

/// Records spawned work instead of running it; [`ManualSpawner::run_pending`]
/// runs it. Lets a test step through intermediate (loading) states
/// deterministically. Cheap-clone (shared), so a test can keep a handle after
/// the spawner is moved into the runtime.
#[derive(Default, Clone)]
pub struct ManualSpawner {
    pending: Rc<RefCell<Vec<(u64, Job)>>>,
    next_id: Rc<Cell<u64>>,
}

enum Job {
    Future(BoxFuture),
    Blocking(BlockingJob),
}

/// Aborting drops the recorded job outright — nothing has run yet, so this is
/// exact rather than best-effort. A job already consumed by `run_pending` is
/// simply absent, and removal is a no-op.
struct ManualHandle {
    pending: Rc<RefCell<Vec<(u64, Job)>>>,
    id: u64,
}

impl TaskHandle for ManualHandle {
    fn abort(&self) {
        self.pending.borrow_mut().retain(|(id, _)| *id != self.id);
    }
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
        let jobs: Vec<(u64, Job)> = std::mem::take(&mut *self.pending.borrow_mut());
        let n = jobs.len();
        for (_, job) in jobs {
            match job {
                Job::Future(fut) => block_on(fut),
                Job::Blocking(f) => f(),
            }
        }
        n
    }

    /// Record `job` under a fresh id and hand back the handle that removes it.
    fn record(&self, job: Job) -> Box<dyn TaskHandle> {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        self.pending.borrow_mut().push((id, job));
        Box::new(ManualHandle {
            pending: Rc::clone(&self.pending),
            id,
        })
    }
}

impl Spawner for ManualSpawner {
    fn spawn(&self, fut: BoxFuture) -> Box<dyn TaskHandle> {
        self.record(Job::Future(fut))
    }
    fn spawn_blocking(&self, f: BlockingJob) -> Box<dyn TaskHandle> {
        self.record(Job::Blocking(f))
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
        // CACHE1: cap the default pool.
        //
        // `available_parallelism()` alone spawns one thread per core — 32 on
        // the dev box — for an app that may never spawn a task. Each carries a
        // stack reservation, so the cost is address space and scheduler
        // pressure paid up front for capacity almost no UI needs. UI work is
        // latency-bound, not throughput-bound: a handful of threads absorbs
        // the IO an app actually offloads, and anything genuinely parallel
        // should ask for a sized pool explicitly via `ThreadPoolSpawner::new`.
        //
        // Matters most on mobile, where core counts are high, memory is tight,
        // and idle threads still cost battery.
        const MAX_DEFAULT_THREADS: usize = 4;
        let n = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(MAX_DEFAULT_THREADS);
        ThreadPoolSpawner::new(n)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ThreadPoolSpawner {
    /// Queue `job` wrapped in a skip check, returning the flag as its handle.
    ///
    /// std's mpsc has no removal, so a queued job cannot be plucked back out —
    /// instead the worker consults the flag before running it, and a job aborted
    /// in time is dropped (releasing the future/closure) rather than executed.
    /// Once a worker is *inside* the job, nothing here can stop it: std cannot
    /// interrupt a thread, so prompt shutdown is up to the job polling
    /// [`Sink::is_cancelled`].
    fn queue(&self, job: BlockingJob) -> Box<dyn TaskHandle> {
        let flag = SkipFlag::default();
        let check = flag.clone();
        let _ = self.tx.send(Box::new(move || {
            if !check.skipped() {
                job();
            }
        }));
        Box::new(flag)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Spawner for ThreadPoolSpawner {
    fn spawn(&self, fut: BoxFuture) -> Box<dyn TaskHandle> {
        self.queue(Box::new(move || block_on(fut)))
    }
    fn spawn_blocking(&self, f: BlockingJob) -> Box<dyn TaskHandle> {
        self.queue(f)
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

    // --- TC1: cancellation ---------------------------------------------------

    #[test]
    fn cancelling_before_the_write_drops_it() {
        let rt = Runtime::new();
        let sig = rt.signal("n", || 0i32);
        let token = CancelToken::new();
        let sink = rt.make_sink_for(None, token.clone());

        token.cancel();
        sink.set(sig, 42);
        rt.drain_deferred();
        assert_eq!(sig.get(&rt), 0, "a cancelled sink never queues the write");
        assert!(sink.is_cancelled(), "the task can see it should stop");
    }

    #[test]
    fn cancelling_after_the_send_still_drops_it_at_apply_time() {
        // The race the send-time check cannot cover: the op is already on the
        // channel when cancellation lands. Without the apply-time guard this op
        // would reach signals that may since have been evicted with their scope.
        let rt = Runtime::new();
        let sig = rt.signal("n", || 0i32);
        let token = CancelToken::new();
        let sink = rt.make_sink_for(None, token.clone());

        sink.set(sig, 42); // queued while still live
        token.cancel(); // ...then cancelled, before the drain
        rt.drain_deferred();
        assert_eq!(sig.get(&rt), 0, "queued op is dropped when applied");
    }

    #[test]
    fn an_uncancelled_sink_is_unaffected() {
        let rt = Runtime::new();
        let sig = rt.signal("n", || 0i32);
        let sink = rt.make_sink_for(None, CancelToken::new());
        sink.set(sig, 42);
        rt.drain_deferred();
        assert_eq!(sig.get(&rt), 42);
    }

    #[test]
    fn manual_spawner_abort_drops_an_unrun_job() {
        let rt = Runtime::new();
        let sig = rt.signal("m", || 0i32);
        let ex = ManualSpawner::new();
        let sink = rt.make_sink();

        let keep = sig;
        let s2 = sink.clone();
        let doomed = ex.spawn_blocking(Box::new(move || sink.set(sig, 5)));
        ex.spawn_blocking(Box::new(move || s2.set(keep, 7)));
        assert_eq!(ex.pending(), 2);

        doomed.abort();
        assert_eq!(ex.pending(), 1, "exactly the aborted job is removed");
        ex.run_pending();
        rt.drain_deferred();
        assert_eq!(sig.get(&rt), 7, "the survivor ran; the aborted one did not");
    }

    #[test]
    fn thread_pool_abort_skips_a_job_no_worker_has_taken() {
        use std::sync::mpsc::channel;
        // One worker, occupied by a job that blocks until we release it, so the
        // second job is provably still queued when we abort it.
        let pool = ThreadPoolSpawner::new(1);
        let (release_tx, release_rx) = channel::<()>();
        let (started_tx, started_rx) = channel();
        pool.spawn_blocking(Box::new(move || {
            started_tx.send(()).unwrap();
            let _ = release_rx.recv();
        }));
        started_rx.recv().unwrap(); // the worker is now busy

        let (ran_tx, ran_rx) = channel();
        let queued = pool.spawn_blocking(Box::new(move || {
            let _ = ran_tx.send(());
        }));
        queued.abort();
        release_tx.send(()).unwrap(); // free the worker

        assert!(
            ran_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .is_err(),
            "the aborted job is skipped rather than run"
        );
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
    static WASM_TASKS: RefCell<Vec<(u64, BoxFuture)>> = const { RefCell::new(Vec::new()) };
    static WASM_NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

/// wasm abort is exact: the queue is ours, so dropping the entry drops the
/// future — the one backend where an in-flight task really stops.
#[cfg(target_arch = "wasm32")]
struct WasmHandle(u64);

#[cfg(target_arch = "wasm32")]
impl TaskHandle for WasmHandle {
    fn abort(&self) {
        WASM_TASKS.with(|q| q.borrow_mut().retain(|(id, _)| *id != self.0));
    }
}

#[cfg(target_arch = "wasm32")]
impl Spawner for WasmSpawner {
    fn spawn(&self, fut: BoxFuture) -> Box<dyn TaskHandle> {
        let id = WASM_NEXT_ID.with(|n| {
            let id = n.get();
            n.set(id + 1);
            id
        });
        WASM_TASKS.with(|q| q.borrow_mut().push((id, fut)));
        Box::new(WasmHandle(id))
    }
    fn spawn_blocking(&self, f: BlockingJob) -> Box<dyn TaskHandle> {
        f(); // single thread: inline (document in the skill; keep jobs small)
        Box::new(NoopHandle)
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
        // Taken, not borrowed across the poll: a future may `spawn` (pushing onto
        // the now-empty queue) or drop a handle, and holding the borrow would
        // panic. The cost is that an `abort` issued *from inside* a future being
        // polled here targets the empty queue and is lost — cancellation
        // normally comes from a handler, which runs in `pump`, not at RAF.
        let mut tasks = q.take();
        tasks.retain_mut(|(_, fut)| fut.as_mut().poll(&mut cx) == Poll::Pending);
        let mut q = q.borrow_mut();
        q.extend(tasks);
        // Counts tasks spawned during this tick too, so the host keeps ticking.
        !q.is_empty()
    })
}
