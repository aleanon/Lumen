//! The reactive data layer: `cx.resource` / `cx.task` (built on
//! `lumen_core::tasks`). A build call *records* a [`TaskRequest`]; the runtime
//! dispatches it after the build on its executor, and results flow back through
//! the deferred-op channel into a backing signal cell — so all state writes
//! happen on the UI thread inside `pump` (determinism preserved).

use crate::element::{BuildCx, TaskRequest};
use lumen_core::state::{Signal, State};
use lumen_core::tasks::Sink;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::future::Future;
use std::hash::{Hash, Hasher};

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
fn finish<T: State + Send, E: State + Send>(
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
        fetch: impl FnOnce(D) -> Fut + Send + 'static,
    ) -> Resource<T, E>
    where
        T: State + Send + Clone,
        E: State + Send + Clone,
        D: Hash + Send + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
    {
        self.resource_impl(key, deps, |deps, sig, gen| {
            TaskRequest::Future(Box::new(move |sink| {
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
        fetch: impl FnOnce(D) -> Result<T, E> + Send + 'static,
    ) -> Resource<T, E>
    where
        T: State + Send + Clone,
        E: State + Send + Clone,
        D: Hash + Send + 'static,
    {
        self.resource_impl(key, deps, |deps, sig, gen| {
            TaskRequest::Blocking(Box::new(move |sink| {
                let r = fetch(deps);
                finish(&sink, sig, gen, r);
            }))
        })
    }

    fn resource_impl<T, E, D>(
        &self,
        key: &str,
        deps: D,
        make_req: impl FnOnce(D, Signal<ResourceCell<T, E>>, u64) -> TaskRequest,
    ) -> Resource<T, E>
    where
        T: State + Clone,
        E: State + Clone,
        D: Hash,
    {
        let dh = hash_deps(&deps);
        let sig = self.signal::<ResourceCell<T, E>>(key, ResourceCell::default);
        let (changed, gen) =
            sig.with(self.runtime(), |c| (!c.started || c.deps_hash != dh, c.gen));
        if changed {
            let new_gen = gen + 1;
            sig.update(self.runtime(), move |c| {
                c.loading = true;
                c.deps_hash = dh;
                c.gen = new_gen;
                c.started = true;
            });
            self.tasks
                .borrow_mut()
                .push(make_req(deps, sig, new_gen));
        }
        sig.with(self.runtime(), |c| Resource {
            value: c.value.clone(),
            error: c.error.clone(),
            loading: c.loading,
        })
    }

    /// Spawn a long-lived async task (e.g. a stream) once per (key, deps). The
    /// closure gets a [`crate::Sink`] to push results back over time (`sink.set` /
    /// `sink.update` a signal). Use for streaming/subscriptions.
    pub fn task<D, Fut>(
        &self,
        key: &str,
        deps: D,
        f: impl FnOnce(D, Sink) -> Fut + Send + 'static,
    ) where
        D: Hash + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.task_impl(key, deps, |deps| {
            TaskRequest::Future(Box::new(move |sink| Box::pin(f(deps, sink))))
        });
    }

    /// Spawn a blocking task (e.g. a heavy compute job streaming progress) once
    /// per (key, deps). The closure gets a [`crate::Sink`] to push results/progress.
    pub fn task_blocking<D>(
        &self,
        key: &str,
        deps: D,
        f: impl FnOnce(D, Sink) + Send + 'static,
    ) where
        D: Hash + Send + 'static,
    {
        self.task_impl(key, deps, |deps| {
            TaskRequest::Blocking(Box::new(move |sink| f(deps, sink)))
        });
    }

    fn task_impl<D>(&self, key: &str, deps: D, make_req: impl FnOnce(D) -> TaskRequest)
    where
        D: Hash,
    {
        let dh = hash_deps(&deps);
        let sig = self.signal::<TaskTracker>(key, TaskTracker::default);
        let changed = sig.with(self.runtime(), |t| !t.started || t.deps_hash != dh);
        if changed {
            sig.update(self.runtime(), move |t| {
                t.deps_hash = dh;
                t.started = true;
            });
            self.tasks.borrow_mut().push(make_req(deps));
        }
    }
}
