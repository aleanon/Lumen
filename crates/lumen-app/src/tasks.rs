//! The reactive data layer: `cx.resource` / `cx.task` (built on
//! `lumen_core::tasks`). A build call *records* a [`TaskRequest`]; the runtime
//! dispatches it after the build on its executor, and results flow back through
//! the deferred-op channel into a backing signal cell — so all state writes
//! happen on the UI thread inside `pump` (determinism preserved).
//!
//! # Lifetime (TC1)
//!
//! Every task is owned by the scope that declared it and is cancelled when that
//! scope leaves the view — a `cx.task` is a subscription with a lifetime, not a
//! fire-and-forget. A change of `deps` likewise cancels the generation it
//! supersedes, so a task never races its own replacement.
//!
//! Cancellation is *always* correct — no write of a cancelled task ever lands —
//! but only sometimes prompt: see [`lumen_core::tasks::TaskHandle`] for what each
//! backend can actually stop. Long-running work should poll
//! [`Sink::is_cancelled`].
//!
//! [`abortable_task`](BuildCx::abortable_task) adds early, on-demand
//! cancellation on top of that lifetime; it never extends it.

use crate::element::{AbortHandle, BuildCx, TaskKind, TaskRequest, TaskSlot};
use lumen_core::state::{Signal, State};
use lumen_core::tasks::{MaybeSend, Sink};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// Default resource error: a message string.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskError(pub String);

impl TaskError {
    /// Build an error from anything string-like.
    pub fn msg(s: impl Into<String>) -> TaskError {
        TaskError(s.into())
    }
}

impl std::fmt::Display for TaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A read-only view of an async resource: the last successful value (which
/// *survives* a refetch or error — stale-while-revalidate), the last error, and
/// whether a fetch is in flight right now. Always show `value` when present;
/// `loading` is an independent indicator.
#[derive(Clone, Debug)]
pub struct Resource<T, E = TaskError> {
    /// Last successful value, or `None` until the first load completes.
    pub value: Option<T>,
    /// Last error (cleared on the next success).
    pub error: Option<E>,
    /// A fetch is currently in flight.
    pub loading: bool,
}

impl<T, E> Resource<T, E> {
    /// Whether a value is available (fresh or stale).
    pub fn is_ready(&self) -> bool {
        self.value.is_some()
    }
}

/// The stored backing state of a resource — one signal cell per key.
#[derive(Clone, Serialize, Deserialize)]
struct ResourceCell<T, E> {
    value: Option<T>,
    error: Option<E>,
    loading: bool,
    deps_hash: u64,
    /// Bumped on each (re)fetch; a result with a stale generation is ignored
    /// (this is how a dep change / drop cancels an in-flight fetch).
    gen: u64,
    started: bool,
}

impl<T, E> Default for ResourceCell<T, E> {
    fn default() -> ResourceCell<T, E> {
        ResourceCell {
            value: None,
            error: None,
            loading: false,
            deps_hash: 0,
            gen: 0,
            started: false,
        }
    }
}

/// Tracks a `task`/`task_blocking` so it is spawned once per (key, deps) rather
/// than every build.
#[derive(Clone, Default, Serialize, Deserialize)]
struct TaskTracker {
    deps_hash: u64,
    started: bool,
}

fn hash_deps(d: &impl Hash) -> u64 {
    let mut h = DefaultHasher::new();
    d.hash(&mut h);
    h.finish()
}

/// Build the deferred op that applies a resource result (guarded by `gen`).
fn finish<T: State + MaybeSend, E: State + MaybeSend>(
    sink: &Sink,
    sig: Signal<ResourceCell<T, E>>,
    gen: u64,
    result: Result<T, E>,
) {
    sink.mutate(move |rt| {
        sig.update(rt, |c| {
            if c.gen != gen {
                return; // stale (deps changed or resource dropped) → ignore
            }
            c.loading = false;
            match result {
                Ok(v) => {
                    c.value = Some(v);
                    c.error = None;
                }
                Err(e) => c.error = Some(e),
            }
        });
    });
}

impl BuildCx<'_> {
    /// Async resource: `fetch(deps)` runs off the UI thread; its result lands in
    /// app state. Re-fetches when `deps` change (the stale value stays visible
    /// while reloading). Keyed by `key` like a signal.
    pub fn resource<T, E, D, Fut>(
        &self,
        key: &str,
        deps: D,
        fetch: impl FnOnce(D) -> Fut + MaybeSend + 'static,
    ) -> Resource<T, E>
    where
        T: State + MaybeSend + Clone,
        E: State + MaybeSend + Clone,
        D: Hash + MaybeSend + 'static,
        Fut: Future<Output = Result<T, E>> + MaybeSend + 'static,
    {
        self.resource_impl(key, deps, |deps, sig, gen| {
            TaskKind::Future(Box::new(move |sink| {
                Box::pin(async move {
                    let r = fetch(deps).await;
                    finish(&sink, sig, gen, r);
                })
            }))
        })
    }

    /// Blocking resource: `fetch(deps)` runs on a pool thread (CPU-bound /
    /// blocking I/O). Same caching/refetch semantics as [`resource`](Self::resource).
    pub fn resource_blocking<T, E, D>(
        &self,
        key: &str,
        deps: D,
        fetch: impl FnOnce(D) -> Result<T, E> + MaybeSend + 'static,
    ) -> Resource<T, E>
    where
        T: State + MaybeSend + Clone,
        E: State + MaybeSend + Clone,
        D: Hash + MaybeSend + 'static,
    {
        self.resource_impl(key, deps, |deps, sig, gen| {
            TaskKind::Blocking(Box::new(move |sink| {
                let r = fetch(deps);
                finish(&sink, sig, gen, r);
            }))
        })
    }

    fn resource_impl<T, E, D>(
        &self,
        key: &str,
        deps: D,
        make_kind: impl FnOnce(D, Signal<ResourceCell<T, E>>, u64) -> TaskKind,
    ) -> Resource<T, E>
    where
        T: State + Clone,
        E: State + Clone,
        D: Hash,
    {
        let dh = hash_deps(&deps);
        let sig: Signal<ResourceCell<T, E>> = self.signal(key, ResourceCell::default);
        let (changed, gen) = sig.with(self.runtime(), |c| (!c.started || c.deps_hash != dh, c.gen));
        if changed {
            let new_gen = gen + 1;
            sig.update(self.runtime(), move |c| {
                c.loading = true;
                c.deps_hash = dh;
                c.gen = new_gen;
                c.started = true;
            });
            // TC1: registering cancels the superseded fetch. The generation guard
            // in `finish` already discarded its *result*; the token additionally
            // stops it burning a thread (where the backend can) — worth having
            // for a row that scrolled off mid-request.
            let (id, slot) = self.register_task(&key);
            self.tasks.borrow_mut().push(TaskRequest {
                id,
                token: slot.token(),
                kind: make_kind(deps, sig, new_gen),
            });
        }
        sig.with(self.runtime(), |c| Resource {
            value: c.value.clone(),
            error: c.error.clone(),
            loading: c.loading,
        })
    }

    /// Spawn a long-lived async task (e.g. a stream) once per (key, deps) — the
    /// framework's subscription primitive. The closure gets a [`Sink`] to
    /// push results back over time (`sink.set` / `sink.update` a signal).
    ///
    /// **Lifetime:** the task lives as long as the scope that declared it. It is
    /// cancelled when that scope leaves the view, and when a change of `deps`
    /// supersedes it. Declaring it inside `cx.scope(item.id, …)` is therefore how
    /// you scope a subscription to a list row or a screen.
    ///
    /// Declaring it at the root ties it to the app's lifetime — an `if` around
    /// the call does **not** stop it, because a task that is simply not
    /// re-declared is not the same thing as one whose scope died. Use
    /// [`abortable_task`](Self::abortable_task) when you want to stop it on
    /// demand.
    ///
    /// Long-running loops should poll [`Sink::is_cancelled`] to stop promptly;
    /// correctness does not depend on it (writes stop landing regardless), but
    /// a pool thread otherwise keeps working for nothing.
    pub fn task<D, Fut>(
        &self,
        key: &str,
        deps: D,
        f: impl FnOnce(D, Sink) -> Fut + MaybeSend + 'static,
    ) where
        D: Hash + MaybeSend + 'static,
        Fut: Future<Output = ()> + MaybeSend + 'static,
    {
        self.task_impl(key, deps, |deps| {
            TaskKind::Future(Box::new(move |sink| Box::pin(f(deps, sink))))
        });
    }

    /// Spawn a blocking task (e.g. a heavy compute job streaming progress) once
    /// per (key, deps). The closure gets a [`Sink`] to push results/progress.
    ///
    /// Same lifetime rules as [`task`](Self::task).
    pub fn task_blocking<D>(
        &self,
        key: &str,
        deps: D,
        f: impl FnOnce(D, Sink) + MaybeSend + 'static,
    ) where
        D: Hash + MaybeSend + 'static,
    {
        self.task_impl(key, deps, |deps| {
            TaskKind::Blocking(Box::new(move |sink| f(deps, sink)))
        });
    }

    /// [`task`](Self::task), plus an [`AbortHandle`] for stopping it on demand.
    ///
    /// The handle is `Rc`-based and cheap to clone, so it captures straight into
    /// a button handler. It cannot be stored in a signal — no handle can — and
    /// does not need to be: re-declaring the task on a later build returns a
    /// handle to the *same* running task, not a new one.
    ///
    /// ```ignore
    /// let dl = cx.abortable_task_blocking("download", (), |_, sink| {
    ///     for chunk in reader {
    ///         if sink.is_cancelled() { break; }
    ///         sink.update(progress, move |p| *p += chunk.len() as u64);
    ///     }
    /// });
    /// widgets::button("Cancel", move |_| dl.abort())
    /// ```
    pub fn abortable_task<D, Fut>(
        &self,
        key: &str,
        deps: D,
        f: impl FnOnce(D, Sink) -> Fut + MaybeSend + 'static,
    ) -> AbortHandle
    where
        D: Hash + MaybeSend + 'static,
        Fut: Future<Output = ()> + MaybeSend + 'static,
    {
        let (slot, fresh) = self.task_impl(key, deps, |deps| {
            TaskKind::Future(Box::new(move |sink| Box::pin(f(deps, sink))))
        });
        self.abortable(key, slot, fresh)
    }

    /// [`task_blocking`](Self::task_blocking), plus an [`AbortHandle`].
    /// See [`abortable_task`](Self::abortable_task).
    pub fn abortable_task_blocking<D>(
        &self,
        key: &str,
        deps: D,
        f: impl FnOnce(D, Sink) + MaybeSend + 'static,
    ) -> AbortHandle
    where
        D: Hash + MaybeSend + 'static,
    {
        let (slot, fresh) = self.task_impl(key, deps, |deps| {
            TaskKind::Blocking(Box::new(move |sink| f(deps, sink)))
        });
        self.abortable(key, slot, fresh)
    }

    /// Pair a task's slot with the scope-local signal that makes its abort
    /// observable. Keyed off the task's own key so it dies with the task.
    ///
    /// `fresh` marks a new generation, which clears the flag: the signal is
    /// keyed by task identity, not by generation, so without this a task
    /// restarted after a cancel would still report itself aborted forever.
    fn abortable(&self, key: &str, slot: Rc<TaskSlot>, fresh: bool) -> AbortHandle {
        let flag = self.signal((key, "lumen.aborted"), || false);
        if fresh && flag.get(self.runtime()) {
            flag.set(self.runtime(), false);
        }
        AbortHandle::new(slot, flag)
    }

    /// Declare a task, returning its slot and whether this call *started* it.
    /// A new `(key, deps)` gets a fresh slot; otherwise the already-running
    /// task's slot comes back, so a re-declaration hands out a handle to the
    /// same task rather than restarting it.
    fn task_impl<D>(
        &self,
        key: &str,
        deps: D,
        make_kind: impl FnOnce(D) -> TaskKind,
    ) -> (Rc<TaskSlot>, bool)
    where
        D: Hash,
    {
        let dh = hash_deps(&deps);
        let sig: Signal<TaskTracker> = self.signal(key, TaskTracker::default);
        let changed = sig.with(self.runtime(), |t| !t.started || t.deps_hash != dh);
        if !changed {
            // Steady state. The slot outlives the request, so it is still here
            // unless the scope died — in which case this build would not be
            // running. Fall back to an inert slot rather than panicking.
            let slot = self
                .lookup_task(&key)
                .unwrap_or_else(|| self.register_task(&key).1);
            return (slot, false);
        }
        sig.update(self.runtime(), move |t| {
            t.deps_hash = dh;
            t.started = true;
        });
        // Registering cancels the previous generation — without this a deps
        // change leaves two tasks writing the same signal.
        let (id, slot) = self.register_task(&key);
        self.tasks.borrow_mut().push(TaskRequest {
            id,
            token: slot.token(),
            kind: make_kind(deps),
        });
        (slot, true)
    }
}
