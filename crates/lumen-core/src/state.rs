//! Signals, the state store, and the checkpoint protocol.
//!
//! Fine-grained reactivity (Solid-style, ADR-007): reading a signal inside a
//! tracking scope subscribes that scope; writing a signal schedules exactly the
//! subscribed scopes — never whole-tree work. Derived [`Memo`]s and [`effect`]s
//! sit on the same graph.
//!
//! The **store is the only retained mutable state** (02 §4). In the default
//! `snapshot` build, stored values are `Serialize + DeserializeOwned`; the
//! reactive graph itself (subscriptions, effect closures) is runtime-only and
//! rebuilt each frame, so a snapshot is pure field-tagged JSON that survives hot
//! reloads and struct evolution (missing fields default, dropped fields warn
//! with `codes::W0002`). A lean build (`--no-default-features`) relaxes the
//! [`State`] bound to `'static`, drops the snapshot API, and unlinks
//! `serde_json` — the same signal source, without the serialization machinery.
//!
//! [`effect`]: Runtime::effect
//!
//! Not yet wired to a consumer (the headless `App`/`BuildCx` arrive in T0.9);
//! `allow(dead_code)` is removed then.

#[cfg(feature = "snapshot")]
use crate::diagnostics::{codes, Diagnostic};
#[cfg(feature = "snapshot")]
use serde::de::DeserializeOwned;
#[cfg(feature = "snapshot")]
use serde::Serialize;
use std::any::Any;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// Anything that can live in the state store.
///
/// With the default `snapshot` feature, stored values are serializable so the
/// whole store can be checkpointed to field-tagged JSON (ADR-011) and read by
/// the agent. In a lean build (`--no-default-features`) the bound relaxes to
/// just `'static`: no per-value serialization, no `serde_json`. The `snapshot`
/// build is the canonical superset — a program that compiles lean also compiles
/// with `snapshot` on, provided its stored types stay serializable, so CI builds
/// the superset.
#[cfg(feature = "snapshot")]
pub trait State: Serialize + DeserializeOwned + 'static {}
#[cfg(feature = "snapshot")]
impl<T: Serialize + DeserializeOwned + 'static> State for T {}

/// Anything that can live in the state store (lean build: `'static` only).
#[cfg(not(feature = "snapshot"))]
pub trait State: 'static {}
#[cfg(not(feature = "snapshot"))]
impl<T: 'static> State for T {}

/// Type-erased stored value: downcast always; serialize only under `snapshot`.
/// Runtime-only (never part of a snapshot), so trait objects are fine here.
trait StoredValue: 'static {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    #[cfg(feature = "snapshot")]
    fn to_json(&self) -> serde_json::Value;
    /// Replace the value in place from a snapshot JSON value (live restore —
    /// the blanket impl knows the concrete `T`, so the type-erased slot can
    /// deserialize). Lenient like creation-time adoption: missing fields
    /// default, dropped fields become `W0002` diagnostics.
    #[cfg(feature = "snapshot")]
    fn restore_json(
        &mut self,
        key: &str,
        json: &serde_json::Value,
    ) -> Result<Vec<Diagnostic>, Diagnostic>;
}
impl<T: State> StoredValue for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    #[cfg(feature = "snapshot")]
    fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
    #[cfg(feature = "snapshot")]
    fn restore_json(
        &mut self,
        key: &str,
        json: &serde_json::Value,
    ) -> Result<Vec<Diagnostic>, Diagnostic> {
        let (t, diags) = deser_lenient::<T>(key, json)?;
        *self = t;
        Ok(diags)
    }
}

/// Interned identity of a stored value (signal or memo). `Copy` so [`Signal`]
/// can be a cheap copyable handle.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct SignalId(u32);

/// Identity of a reactive scope (effect or memo recompute).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct ScopeId(u32);

/// A copyable handle to a stored signal value (02 §4).
pub struct Signal<T> {
    id: SignalId,
    _pd: PhantomData<fn() -> T>,
}
impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Signal<T> {}

/// A copyable handle to a derived (memoized) value (02 §4).
pub struct Memo<T> {
    id: SignalId,
    _pd: PhantomData<fn() -> T>,
}
impl<T> Clone for Memo<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Memo<T> {}

/// The loading state of an async [`Resource`].
#[derive(Clone)]
#[cfg_attr(feature = "snapshot", derive(serde::Serialize, serde::Deserialize))]
pub enum ResourceState<T> {
    /// The backing future has not yet resolved.
    Loading,
    /// The future resolved to a value.
    Ready(T),
}

/// A handle to an async resource (02 §4). Full polling integration lands with
/// the shell's runtime (T0.9 / ADR-018); creation polls the future once.
pub struct Resource<T> {
    sig: Signal<ResourceState<T>>,
}
impl<T> Clone for Resource<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Resource<T> {}

/// Read access to the store. Implemented by [`Runtime`] (untracked) and
/// [`ReadScope`] (tracked — subscribes the running scope).
pub trait ReadCx {
    #[doc(hidden)]
    fn runtime(&self) -> &Runtime;
    #[doc(hidden)]
    fn tracks(&self) -> bool;
}

/// Write access to the store. Implemented by [`Runtime`] and [`ReadScope`].
pub trait WriteCx {
    #[doc(hidden)]
    fn runtime(&self) -> &Runtime;
}

/// The tracked read/write context handed to effect and memo closures.
pub struct ReadScope {
    rt: Runtime,
}
impl ReadCx for ReadScope {
    fn runtime(&self) -> &Runtime {
        &self.rt
    }
    fn tracks(&self) -> bool {
        true
    }
}
impl WriteCx for ReadScope {
    fn runtime(&self) -> &Runtime {
        &self.rt
    }
}

struct Slot {
    value: Box<dyn StoredValue>,
    subs: HashSet<ScopeId>,
    /// The `write_gen` at this value's last write (0 = never written since
    /// creation). A [`ReadSet`] records per-signal versions so a memoized view
    /// scope can tell whether *its* deps changed — finer than the global
    /// `write_gen`, which only says *something* changed.
    version: u64,
}

struct ScopeData {
    deps: HashSet<SignalId>,
    run: Rc<dyn Fn(&ReadScope)>,
}

#[derive(Default)]
struct Inner {
    slots: HashMap<SignalId, Slot>,
    scopes: HashMap<ScopeId, ScopeData>,

    // interning: stable string key <-> dense id
    key_to_id: HashMap<String, SignalId>,
    id_to_key: Vec<String>,
    scope_key_to_id: HashMap<String, ScopeId>,
    next_scope: u32,

    // reactive bookkeeping
    stack: Vec<ScopeId>,
    dirty: Vec<ScopeId>,
    dirty_set: HashSet<ScopeId>,
    batch_depth: u32,
    run_counter: u64,
    /// Active read-collection windows ([`Runtime::collect_reads`]). Each signal
    /// read pushes its id onto the top window, so a memoized view scope learns
    /// exactly which signals it depends on. A stack so nested scopes attribute
    /// reads to the innermost scope only (correct fine-grained nesting).
    read_collectors: Vec<Vec<SignalId>>,
    /// Bumped on every value write (signal `set`, or a memo whose value actually
    /// changed). The runtime compares it across frames to skip a rebuild when no
    /// state changed since the last one. Conservative: `set` bumps even when the
    /// written value equals the old one.
    write_gen: u64,

    // restore
    #[cfg(feature = "snapshot")]
    pending: HashMap<String, serde_json::Value>,
    /// Host mailbox (W.2): transient messages from handlers to the host
    /// (e.g. `SystemRequest`s). Runtime-internal — never part of a snapshot,
    /// unlike the store — so posting one can't create `W0002` churn on
    /// tier-3 restore.
    posted: Vec<Box<dyn std::any::Any>>,
    #[cfg(feature = "snapshot")]
    restore_diags: Vec<Diagnostic>,
}

/// The signals read during a [`Runtime::collect_reads`] window, each paired with
/// its value-version at capture time. Lets a memoized view scope (F1) decide
/// whether to re-run: it is *current* while none of those signals has been
/// written since. Empty ⇒ the scope read no state (always current).
#[derive(Clone, Default)]
pub struct ReadSet {
    deps: Vec<(SignalId, u64)>,
}

impl ReadSet {
    /// True while every captured signal still holds the version it had at
    /// capture — i.e. none has been written since. A written (or dropped) dep
    /// makes this false, so the owning scope must re-run.
    pub fn is_current(&self, rt: &Runtime) -> bool {
        let b = rt.inner.borrow();
        self.deps
            .iter()
            .all(|(id, ver)| b.slots.get(id).map(|s| s.version) == Some(*ver))
    }

    /// Whether the scope read no signals at all (a constant subtree).
    pub fn is_empty(&self) -> bool {
        self.deps.is_empty()
    }

    /// Merge another read set's deps in (dedup by signal id), for building a
    /// combined "structural" read set from several sources (F3).
    pub fn extend(&mut self, other: &ReadSet) {
        for &(id, ver) in &other.deps {
            if !self.deps.iter().any(|(i, _)| *i == id) {
                self.deps.push((id, ver));
            }
        }
    }

    /// The stable string keys of the signals captured, for observability — a
    /// scope's dependency list projected into the agent's view (F2). Order
    /// follows first-read; unknown ids (dropped) are skipped.
    pub fn dep_keys(&self, rt: &Runtime) -> Vec<String> {
        let b = rt.inner.borrow();
        self.deps
            .iter()
            .filter_map(|(id, _)| b.id_to_key.get(id.0 as usize).cloned())
            .collect()
    }
}

/// A self-describing snapshot of the entire store (ADR-011): field-tagged JSON,
/// keyed by each value's stable string key.
#[cfg(feature = "snapshot")]
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct StateSnapshot(pub serde_json::Value);

/// The reactive runtime and state store. Cheap to clone (shared, interior
/// mutability) so it can be handed to read/write contexts.
#[derive(Clone)]
pub struct Runtime {
    inner: Rc<RefCell<Inner>>,
    /// Channel for off-thread results (the data layer); see [`crate::tasks`].
    deferred: Rc<crate::tasks::DeferredChannel>,
    /// Shared clipboard text, reachable from event handlers (which only get a
    /// `&Runtime`) — text widgets cut/copy/paste through it. The desktop shell
    /// syncs it with the OS clipboard.
    clipboard: Rc<RefCell<String>>,
    /// Diagnostic log ring (C.2): `(next_seq, entries)`, capped at 1000.
    /// Reachable from handlers and builds (a side-channel that never feeds
    /// rendering); the agent reads it via `app.logs`.
    logs: Rc<RefCell<(u64, std::collections::VecDeque<LogEntry>)>>,
}

/// A diagnostic log entry (C.2) — agent-visible via the protocol's
/// `app.logs {since}`.
#[derive(Clone, Debug)]
pub struct LogEntry {
    /// Monotonic per-runtime sequence number.
    pub seq: u64,
    /// `"info" | "warn" | "error"`.
    pub level: &'static str,
    /// The message text.
    pub message: String,
}

impl Default for Runtime {
    fn default() -> Runtime {
        Runtime::new()
    }
}

impl ReadCx for Runtime {
    fn runtime(&self) -> &Runtime {
        self
    }
    fn tracks(&self) -> bool {
        false
    }
}
impl WriteCx for Runtime {
    fn runtime(&self) -> &Runtime {
        self
    }
}

impl Runtime {
    /// A fresh, empty runtime.
    pub fn new() -> Runtime {
        Runtime {
            inner: Rc::new(RefCell::new(Inner::default())),
            deferred: Rc::new(crate::tasks::DeferredChannel::new()),
            clipboard: Rc::new(RefCell::new(String::new())),
            logs: Rc::new(RefCell::new((0, std::collections::VecDeque::new()))),
        }
    }

    /// Append a diagnostic log entry (C.2). Callable from handlers and builds
    /// — a side-channel that never feeds rendering, so build purity holds.
    /// Ring-buffered: the oldest entry drops past 1000.
    pub fn log(&self, level: &'static str, message: impl Into<String>) {
        let mut l = self.logs.borrow_mut();
        let seq = l.0;
        l.0 += 1;
        if l.1.len() >= 1000 {
            l.1.pop_front();
        }
        l.1.push_back(LogEntry {
            seq,
            level,
            message: message.into(),
        });
    }

    /// Entries with `seq >= since`, oldest first (C.2; the agent's
    /// `app.logs {since}` — page by passing the last seen seq + 1).
    pub fn logs_since(&self, since: u64) -> Vec<LogEntry> {
        self.logs
            .borrow()
            .1
            .iter()
            .filter(|e| e.seq >= since)
            .cloned()
            .collect()
    }

    /// The deferred-op channel (data layer). Internal accessor for `tasks`.
    pub(crate) fn deferred(&self) -> &crate::tasks::DeferredChannel {
        &self.deferred
    }

    /// The current clipboard text. Shared across handler closures (which only
    /// receive `&Runtime`); the shell keeps it in sync with the OS clipboard.
    pub fn clipboard(&self) -> String {
        self.clipboard.borrow().clone()
    }

    /// Replace the clipboard text (e.g. a text widget's copy/cut).
    pub fn set_clipboard(&self, text: impl Into<String>) {
        *self.clipboard.borrow_mut() = text.into();
    }

    /// Total number of scope runs since creation — used by tests to assert that
    /// a write re-runs *exactly* the subscribed scopes.
    pub fn run_count(&self) -> u64 {
        self.inner.borrow().run_counter
    }

    /// A monotonic counter bumped on every value write (signal `set`, or a memo
    /// whose value changed). The runtime compares it across frames to skip a
    /// rebuild when nothing changed since the last one.
    pub fn write_gen(&self) -> u64 {
        self.inner.borrow().write_gen
    }

    /// True when no reactive scope is pending — the graph has reached a fixpoint.
    /// A settled `pump` must leave the runtime quiescent (the F0 contract): all
    /// writes flush synchronously, so once event dispatch + build finish, nothing
    /// should remain dirty.
    pub fn is_quiescent(&self) -> bool {
        self.inner.borrow().dirty.is_empty()
    }

    /// Drop every stored signal whose key starts with `prefix` (F5 list GC): a
    /// keyed scope that vanished this build sheds its scope-local state, so a
    /// churning list doesn't leak slots. The interned key↔id mapping is kept
    /// (cheap), so re-adding the same key re-creates the slot from its
    /// initializer. Returns how many slots were removed.
    pub fn evict_prefix(&self, prefix: &str) -> usize {
        let mut b = self.inner.borrow_mut();
        let ids: Vec<SignalId> = b
            .key_to_id
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(_, id)| *id)
            .collect();
        let mut n = 0;
        for id in ids {
            if b.slots.remove(&id).is_some() {
                n += 1;
            }
        }
        n
    }

    /// Run `f`, recording every signal it reads, and return the result plus a
    /// [`ReadSet`] capturing those signals at their current versions (F1). A
    /// memoized view scope re-runs only when `ReadSet::is_current` turns false —
    /// i.e. one of the signals it read has since been written. Nesting is
    /// correct: reads inside an inner `collect_reads` attribute to the inner
    /// window only, so a parent scope isn't invalidated by a child's dep.
    pub fn collect_reads<R>(&self, f: impl FnOnce() -> R) -> (R, ReadSet) {
        self.inner.borrow_mut().read_collectors.push(Vec::new());
        let r = f();
        let ids = self
            .inner
            .borrow_mut()
            .read_collectors
            .pop()
            .unwrap_or_default();
        (r, self.snapshot_reads(ids))
    }

    /// Like [`collect_reads`](Self::collect_reads), but hides the reads from any
    /// *enclosing* window — an isolated reactive boundary (a paint-only prop
    /// binding, F3). Its deps belong to the binding alone, not the surrounding
    /// scope/structural collector, so a change to them can patch that one prop
    /// without re-running the build.
    pub fn collect_reads_isolated<R>(&self, f: impl FnOnce() -> R) -> (R, ReadSet) {
        // Detach the outer stack so reads don't propagate up; run with one fresh
        // window; restore the outer stack.
        let outer = std::mem::take(&mut self.inner.borrow_mut().read_collectors);
        self.inner.borrow_mut().read_collectors.push(Vec::new());
        let r = f();
        let ids = self
            .inner
            .borrow_mut()
            .read_collectors
            .pop()
            .unwrap_or_default();
        self.inner.borrow_mut().read_collectors = outer;
        (r, self.snapshot_reads(ids))
    }

    /// Re-notify the currently-open collectors of a previously-captured read
    /// set. Used when a memoized scope is *skipped* (F1): its closure doesn't
    /// run, but its deps must still reach the enclosing scope / structural window
    /// (F3.4), since a change to them still requires re-running it.
    pub fn replay_reads(&self, reads: &ReadSet) {
        let mut b = self.inner.borrow_mut();
        if b.read_collectors.is_empty() {
            return;
        }
        let ids: Vec<SignalId> = reads.deps.iter().map(|(id, _)| *id).collect();
        for win in b.read_collectors.iter_mut() {
            win.extend(ids.iter().copied());
        }
    }

    /// Stamp a list of read signal ids with their current versions (dedup).
    fn snapshot_reads(&self, ids: Vec<SignalId>) -> ReadSet {
        let b = self.inner.borrow();
        let mut seen = HashSet::new();
        let deps: Vec<(SignalId, u64)> = ids
            .into_iter()
            .filter(|id| seen.insert(*id))
            .map(|id| (id, b.slots.get(&id).map(|s| s.version).unwrap_or(0)))
            .collect();
        ReadSet { deps }
    }

    /// Number of stored values.
    pub fn len(&self) -> usize {
        self.inner.borrow().slots.len()
    }

    /// Whether the store holds no values.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // --- creation -----------------------------------------------------------

    /// Create or re-attach a signal. The key is the identity path + name (02
    /// §4); on restore, a staged snapshot value is adopted instead of `init`.
    // Not the `entry` pattern: the slot value is built from `init`/`pending`,
    // and `b` is borrowed for the pending map in between.
    #[allow(clippy::map_entry)]
    pub fn signal<T: State>(&self, key: &str, init: impl FnOnce() -> T) -> Signal<T> {
        let id = self.intern(key);
        let mut b = self.inner.borrow_mut();
        if !b.slots.contains_key(&id) {
            // On restore, adopt a staged snapshot value instead of `init`.
            #[cfg(feature = "snapshot")]
            let value: Box<dyn StoredValue> = match b.pending.remove(key) {
                Some(json) => match deser_lenient::<T>(key, &json) {
                    Ok((t, diags)) => {
                        b.restore_diags.extend(diags);
                        Box::new(t)
                    }
                    Err(d) => {
                        b.restore_diags.push(d);
                        Box::new(init())
                    }
                },
                None => Box::new(init()),
            };
            #[cfg(not(feature = "snapshot"))]
            let value: Box<dyn StoredValue> = Box::new(init());
            b.slots.insert(
                id,
                Slot {
                    value,
                    subs: HashSet::new(),
                    version: 0,
                },
            );
        }
        Signal {
            id,
            _pd: PhantomData,
        }
    }

    /// Register (or replace) an effect: a scope that re-runs whenever any signal
    /// it read changes. Runs once immediately to establish subscriptions.
    pub fn effect(&self, key: &str, f: impl Fn(&ReadScope) + 'static) {
        let id = self.intern_scope(key);
        {
            let mut b = self.inner.borrow_mut();
            b.scopes.insert(
                id,
                ScopeData {
                    deps: HashSet::new(),
                    run: Rc::new(f),
                },
            );
        }
        self.run_scope(id);
    }

    /// Create or re-attach a memo: a derived value recomputed when its
    /// dependencies change, notifying *its* subscribers only when the value
    /// actually changes (`PartialEq`).
    pub fn memo<T: PartialEq + State>(
        &self,
        key: &str,
        f: impl Fn(&ReadScope) -> T + 'static,
    ) -> Memo<T> {
        let value_id = self.intern(key);
        let scope_id = self.intern_scope(&format!("memo:{key}"));
        let rt = self.clone();
        let run = move |scope: &ReadScope| {
            let v = f(scope);
            rt.update_memo_value::<T>(value_id, v);
        };
        {
            let mut b = self.inner.borrow_mut();
            b.scopes.insert(
                scope_id,
                ScopeData {
                    deps: HashSet::new(),
                    run: Rc::new(run),
                },
            );
        }
        self.run_scope(scope_id);
        Memo {
            id: value_id,
            _pd: PhantomData,
        }
    }

    /// Create an async resource. The future is polled once now; if it does not
    /// resolve immediately the resource stays [`ResourceState::Loading`] until
    /// the shell's executor drives it (T0.9).
    pub fn resource<T: State>(
        &self,
        key: &str,
        fut: impl std::future::Future<Output = T> + 'static,
    ) -> Resource<T> {
        let sig = self.signal::<ResourceState<T>>(key, || ResourceState::Loading);
        let mut fut = Box::pin(fut);
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            sig.set(self, ResourceState::Ready(v));
        }
        Resource { sig }
    }

    /// Run `f` with writes batched: subscribed scopes flush once, after `f`
    /// returns, instead of after each write.
    pub fn batch<R>(&self, f: impl FnOnce() -> R) -> R {
        self.inner.borrow_mut().batch_depth += 1;
        let r = f();
        let flush = {
            let mut b = self.inner.borrow_mut();
            b.batch_depth -= 1;
            b.batch_depth == 0
        };
        if flush {
            self.flush();
        }
        r
    }

    // --- snapshot / restore (Checkpoint pieces) -----------------------------

    /// Serialize the whole store to field-tagged JSON keyed by stable string key.
    #[cfg(feature = "snapshot")]
    pub fn snapshot(&self) -> StateSnapshot {
        let b = self.inner.borrow();
        let mut map = serde_json::Map::new();
        for (id, slot) in &b.slots {
            let key = b.id_to_key[id.0 as usize].clone();
            map.insert(key, slot.value.to_json());
        }
        StateSnapshot(serde_json::Value::Object(map))
    }

    /// Stage a snapshot for restoration. Values are adopted as signals are
    /// (re-)created; call [`Runtime::finish_restore`] afterward to collect
    /// `W0002` diagnostics for fields/keys that no longer exist.
    #[cfg(feature = "snapshot")]
    pub fn load_pending(&self, snap: StateSnapshot) {
        let mut b = self.inner.borrow_mut();
        b.pending.clear();
        b.restore_diags.clear();
        if let serde_json::Value::Object(map) = snap.0 {
            for (k, v) in map {
                b.pending.insert(k, v);
            }
        }
    }

    /// Finish a restore: returns accumulated `W0002` diagnostics, including one
    /// per snapshot key that was never re-attached (whole dropped value).
    #[cfg(feature = "snapshot")]
    pub fn finish_restore(&self) -> Vec<Diagnostic> {
        let mut b = self.inner.borrow_mut();
        let mut diags = std::mem::take(&mut b.restore_diags);
        let leftover: Vec<String> = b.pending.keys().cloned().collect();
        for k in leftover {
            diags.push(Diagnostic::new(
                codes::W0002,
                format!("dropped state value `{k}` (no longer present after restore)"),
            ));
        }
        b.pending.clear();
        diags
    }

    /// Adopt staged snapshot values into **existing** slots, in place (the
    /// live-restore half of the Checkpoint protocol — creation-time adoption
    /// in [`Runtime::signal`] only covers slots created *after*
    /// [`Runtime::load_pending`]). Each adopted value schedules its
    /// subscribers exactly like a normal write; keys with no live slot stay
    /// pending for signals the next rebuild re-creates.
    #[cfg(feature = "snapshot")]
    pub fn adopt_pending_live(&self) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        let adopted = {
            let mut borrow = self.inner.borrow_mut();
            let b = &mut *borrow;
            let mut any = false;
            for (id, slot) in b.slots.iter_mut() {
                let key = &b.id_to_key[id.0 as usize];
                let Some(json) = b.pending.remove(key) else {
                    continue;
                };
                match slot.value.restore_json(key, &json) {
                    Ok(d) => diags.extend(d),
                    Err(d) => {
                        diags.push(d);
                        continue;
                    }
                }
                let ver = b.write_gen.wrapping_add(1);
                b.write_gen = ver;
                slot.version = ver;
                for s in slot.subs.iter().copied().collect::<Vec<_>>() {
                    if b.dirty_set.insert(s) {
                        b.dirty.push(s);
                    }
                }
                any = true;
            }
            any
        };
        if adopted {
            self.flush();
        }
        diags
    }

    /// Post a transient message to the host mailbox (W.2) — the channel for
    /// handler-side requests that must reach the host loop (the widget layer
    /// drains `SystemRequest`s each pump). Not reactive, not snapshotted.
    pub fn post<T: 'static>(&self, msg: T) {
        self.inner.borrow_mut().posted.push(Box::new(msg));
    }

    /// Take every posted message of type `T`, preserving order; other types
    /// stay queued.
    pub fn take_posted<T: 'static>(&self) -> Vec<T> {
        let mut b = self.inner.borrow_mut();
        let mut out = Vec::new();
        let mut keep = Vec::new();
        for item in b.posted.drain(..) {
            match item.downcast::<T>() {
                Ok(v) => out.push(*v),
                Err(other) => keep.push(other),
            }
        }
        b.posted = keep;
        out
    }

    // --- internals ----------------------------------------------------------

    fn intern(&self, key: &str) -> SignalId {
        let mut b = self.inner.borrow_mut();
        if let Some(&id) = b.key_to_id.get(key) {
            return id;
        }
        let id = SignalId(b.id_to_key.len() as u32);
        b.id_to_key.push(key.to_string());
        b.key_to_id.insert(key.to_string(), id);
        id
    }

    fn intern_scope(&self, key: &str) -> ScopeId {
        let mut b = self.inner.borrow_mut();
        if let Some(&id) = b.scope_key_to_id.get(key) {
            return id;
        }
        let id = ScopeId(b.next_scope);
        b.next_scope += 1;
        b.scope_key_to_id.insert(key.to_string(), id);
        id
    }

    /// Subscribe the currently-running scope (if any) to `id`.
    fn track(&self, id: SignalId) {
        let mut b = self.inner.borrow_mut();
        let Some(&scope) = b.stack.last() else {
            return;
        };
        if let Some(slot) = b.slots.get_mut(&id) {
            slot.subs.insert(scope);
        }
        if let Some(sd) = b.scopes.get_mut(&scope) {
            sd.deps.insert(id);
        }
    }

    /// Record a read into *every* open [`Runtime::collect_reads`] window (no-op
    /// when none is open — the common case during an untracked build). Reads
    /// propagate to all enclosing windows, not just the innermost, so a memoized
    /// outer scope is invalidated when an *inner* scope's dep changes — its
    /// cached subtree embeds the inner one. (The inner scope still skips
    /// independently when only a cousin changed: its own window saw only its own
    /// reads.)
    fn note_read(&self, id: SignalId) {
        let mut b = self.inner.borrow_mut();
        for win in b.read_collectors.iter_mut() {
            win.push(id);
        }
    }

    fn read_with<T: 'static, R>(
        &self,
        cx: &impl ReadCx,
        id: SignalId,
        f: impl FnOnce(&T) -> R,
    ) -> R {
        if cx.tracks() {
            self.track(id);
        }
        self.note_read(id);
        let b = self.inner.borrow();
        let slot = b.slots.get(&id).expect("signal slot missing");
        let v = slot
            .value
            .as_any()
            .downcast_ref::<T>()
            .expect("signal type mismatch");
        f(v)
    }

    fn set_value<T: State>(&self, id: SignalId, value: T) {
        let batching = {
            let mut b = self.inner.borrow_mut();
            let ver = b.write_gen.wrapping_add(1);
            b.write_gen = ver;
            if let Some(slot) = b.slots.get_mut(&id) {
                slot.value = Box::new(value);
                slot.version = ver;
            }
            let subs: Vec<ScopeId> = b
                .slots
                .get(&id)
                .map(|s| s.subs.iter().copied().collect())
                .unwrap_or_default();
            for s in subs {
                if b.dirty_set.insert(s) {
                    b.dirty.push(s);
                }
            }
            b.batch_depth > 0
        };
        if !batching {
            self.flush();
        }
    }

    fn update_memo_value<T: PartialEq + State>(&self, id: SignalId, value: T) {
        // Memo recompute runs mid-flush: enqueue dependents but never flush here.
        let mut b = self.inner.borrow_mut();
        let changed = match b.slots.get(&id) {
            Some(slot) => slot
                .value
                .as_any()
                .downcast_ref::<T>()
                .map(|cur| *cur != value)
                .unwrap_or(true),
            None => true,
        };
        if !changed {
            return;
        }
        let ver = b.write_gen.wrapping_add(1);
        b.write_gen = ver;
        let subs: Vec<ScopeId> = match b.slots.get_mut(&id) {
            Some(slot) => {
                slot.value = Box::new(value);
                slot.version = ver;
                slot.subs.iter().copied().collect()
            }
            None => {
                b.slots.insert(
                    id,
                    Slot {
                        value: Box::new(value),
                        subs: HashSet::new(),
                        version: ver,
                    },
                );
                Vec::new()
            }
        };
        for s in subs {
            if b.dirty_set.insert(s) {
                b.dirty.push(s);
            }
        }
    }

    fn flush(&self) {
        loop {
            let id = {
                let mut b = self.inner.borrow_mut();
                if b.batch_depth > 0 || b.dirty.is_empty() {
                    return;
                }
                let id = b.dirty.remove(0);
                b.dirty_set.remove(&id);
                id
            };
            self.run_scope(id);
        }
    }

    fn run_scope(&self, id: ScopeId) {
        let (run, deps) = {
            let mut b = self.inner.borrow_mut();
            let Some(sd) = b.scopes.get_mut(&id) else {
                return;
            };
            (sd.run.clone(), std::mem::take(&mut sd.deps))
        };
        {
            let mut b = self.inner.borrow_mut();
            for k in &deps {
                if let Some(slot) = b.slots.get_mut(k) {
                    slot.subs.remove(&id);
                }
            }
            b.stack.push(id);
            b.run_counter += 1;
        }
        let scope = ReadScope { rt: self.clone() };
        run(&scope);
        self.inner.borrow_mut().stack.pop();
    }
}

impl<T: State> Signal<T> {
    /// Read a clone of the value (subscribes if `cx` tracks).
    pub fn get(&self, cx: &impl ReadCx) -> T
    where
        T: Clone,
    {
        self.with(cx, |v| v.clone())
    }

    /// Read the value by reference (subscribes if `cx` tracks).
    pub fn with<R>(&self, cx: &impl ReadCx, f: impl FnOnce(&T) -> R) -> R {
        cx.runtime().read_with(cx, self.id, f)
    }

    /// Replace the value, scheduling subscribed scopes.
    pub fn set(&self, cx: &impl WriteCx, value: T) {
        cx.runtime().set_value(self.id, value);
    }

    /// Mutate the value in place, then schedule subscribed scopes.
    ///
    /// The closure receives `&mut T` and runs while the store is borrowed, so it
    /// must not read or write *other* signals (doing so re-enters the runtime and
    /// panics on the borrow). Keep it a pure mutation of this value —
    /// `|v| v.push(x)`. This is O(1) in the value's size (an in-place edit); it
    /// does not clone the value.
    pub fn update(&self, cx: &impl WriteCx, f: impl FnOnce(&mut T)) {
        let rt = cx.runtime();
        let batching = {
            let mut b = rt.inner.borrow_mut();
            let ver = b.write_gen.wrapping_add(1);
            b.write_gen = ver;
            {
                let slot = b.slots.get_mut(&self.id).expect("signal slot missing");
                slot.version = ver;
                let v = slot
                    .value
                    .as_any_mut()
                    .downcast_mut::<T>()
                    .expect("signal type mismatch");
                f(v);
            }
            let subs: Vec<ScopeId> = b
                .slots
                .get(&self.id)
                .map(|s| s.subs.iter().copied().collect())
                .unwrap_or_default();
            for s in subs {
                if b.dirty_set.insert(s) {
                    b.dirty.push(s);
                }
            }
            b.batch_depth > 0
        };
        if !batching {
            rt.flush();
        }
    }
}

impl<T: State + Clone> Memo<T> {
    /// Read the current derived value (subscribes if `cx` tracks).
    pub fn get(&self, cx: &impl ReadCx) -> T {
        self.with(cx, |v| v.clone())
    }
    /// Read the derived value by reference.
    pub fn with<R>(&self, cx: &impl ReadCx, f: impl FnOnce(&T) -> R) -> R {
        cx.runtime().read_with(cx, self.id, f)
    }
}

impl<T: State + Clone> Resource<T> {
    /// The current resource state.
    pub fn get(&self, cx: &impl ReadCx) -> ResourceState<T> {
        self.sig.get(cx)
    }
}

/// Deserialize a snapshot value into `T`, tolerating missing fields (via the
/// type's `serde(default)`) and reporting dropped (now-unknown) fields as
/// `W0002`. On hard failure, returns a single `W0002` so the caller can fall
/// back to the initializer.
#[cfg(feature = "snapshot")]
fn deser_lenient<T: State>(
    key: &str,
    json: &serde_json::Value,
) -> Result<(T, Vec<Diagnostic>), Diagnostic> {
    match serde_json::from_value::<T>(json.clone()) {
        Ok(t) => {
            let mut diags = Vec::new();
            if let serde_json::Value::Object(orig) = json {
                if let Ok(serde_json::Value::Object(reser)) = serde_json::to_value(&t) {
                    for k in orig.keys() {
                        if !reser.contains_key(k) {
                            diags.push(Diagnostic::new(
                                codes::W0002,
                                format!("dropped state field `{k}` while restoring `{key}`"),
                            ));
                        }
                    }
                }
            }
            Ok((t, diags))
        }
        Err(e) => Err(Diagnostic::new(
            codes::W0002,
            format!("could not restore `{key}` ({e}); using default"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "snapshot")]
    use serde::Deserialize;
    use std::cell::Cell;

    #[test]
    fn clipboard_is_shared_across_runtime_clones() {
        // Handlers capture clones of the Runtime; they must see the same buffer.
        let rt = Runtime::new();
        assert_eq!(rt.clipboard(), "");
        let handle = rt.clone();
        handle.set_clipboard("copied");
        assert_eq!(rt.clipboard(), "copied");
        rt.set_clipboard(String::from("replaced"));
        assert_eq!(handle.clipboard(), "replaced");
    }

    #[test]
    fn write_one_of_many_reruns_exactly_one_scope() {
        let rt = Runtime::new();
        const N: usize = 10_000;
        let sigs: Vec<Signal<i32>> = (0..N).map(|i| rt.signal(&format!("s{i}"), || 0)).collect();
        let counter = Rc::new(Cell::new(0u64));
        for (i, &s) in sigs.iter().enumerate() {
            let c = counter.clone();
            rt.effect(&format!("e{i}"), move |scope| {
                let _ = s.get(scope); // subscribe to exactly this signal
                c.set(c.get() + 1);
            });
        }
        // Each effect ran once on registration.
        assert_eq!(counter.get(), N as u64);
        let before = rt.run_count();
        sigs[1234].set(&rt, 42);
        // Exactly one scope re-ran.
        assert_eq!(rt.run_count() - before, 1);
        assert_eq!(counter.get(), N as u64 + 1);
        // Writing an unrelated signal also re-runs exactly its own scope.
        let before = rt.run_count();
        sigs[42].set(&rt, 7);
        assert_eq!(rt.run_count() - before, 1);
    }

    #[test]
    fn untracked_read_does_not_subscribe() {
        let rt = Runtime::new();
        let s = rt.signal("s", || 1i32);
        let runs = Rc::new(Cell::new(0u64));
        let r = runs.clone();
        let rt_untracked = rt.clone();
        // The effect reads through the Runtime (untracked) rather than the
        // tracking ReadScope, so it must NOT subscribe or re-run on writes.
        rt.effect("e", move |_scope| {
            let _ = s.get(&rt_untracked);
            r.set(r.get() + 1);
        });
        let before = runs.get();
        s.set(&rt, 2);
        assert_eq!(runs.get(), before, "untracked effect must not re-run");
    }

    #[test]
    fn memo_recomputes_and_caches() {
        let rt = Runtime::new();
        let a = rt.signal("a", || 2i32);
        let m = rt.memo("double", move |scope| a.get(scope) * 2);
        assert_eq!(m.get(&rt), 4);
        a.set(&rt, 5);
        assert_eq!(m.get(&rt), 10);
    }

    #[cfg(feature = "snapshot")]
    #[test]
    fn snapshot_restore_is_lossless_for_1k_signals() {
        let rt = Runtime::new();
        const N: i64 = 1000;
        for i in 0..N {
            rt.signal(&format!("k{i}"), || i * 3);
        }
        let snap = rt.snapshot();

        let rt2 = Runtime::new();
        rt2.load_pending(snap);
        let restored: Vec<Signal<i64>> = (0..N)
            .map(|i| rt2.signal(&format!("k{i}"), || -1)) // init must be ignored
            .collect();
        for (i, &s) in restored.iter().enumerate() {
            assert_eq!(
                s.get(&rt2),
                i as i64 * 3,
                "value {i} not restored losslessly"
            );
        }
        assert!(rt2.finish_restore().is_empty(), "no diagnostics expected");
    }

    #[cfg(feature = "snapshot")]
    #[test]
    fn struct_evolution_defaults_missing_and_warns_dropped() {
        #[derive(Serialize, Deserialize)]
        struct Old {
            a: i32,
            b: i32,
        }
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        #[serde(default)]
        struct New {
            a: i32,
            c: i32, // added
        }
        impl Default for New {
            fn default() -> Self {
                New { a: 0, c: 99 }
            }
        }

        let rt = Runtime::new();
        rt.signal("user", || Old { a: 1, b: 2 });
        let snap = rt.snapshot();

        let rt2 = Runtime::new();
        rt2.load_pending(snap);
        let s = rt2.signal("user", New::default);
        // `a` carried over; `c` defaulted; `b` dropped.
        s.with(&rt2, |v: &New| {
            assert_eq!(v.a, 1, "kept field");
            assert_eq!(v.c, 99, "missing new field defaulted");
        });
        let diags = rt2.finish_restore();
        assert!(
            diags
                .iter()
                .any(|d| d.code == codes::W0002 && d.message.contains('b')),
            "expected W0002 for dropped field `b`, got: {diags:?}"
        );
    }

    #[test]
    fn batch_flushes_once() {
        let rt = Runtime::new();
        let a = rt.signal("a", || 0i32);
        let runs = Rc::new(Cell::new(0u64));
        let r = runs.clone();
        rt.effect("e", move |scope| {
            let _ = a.get(scope);
            r.set(r.get() + 1);
        });
        let before = runs.get();
        rt.batch(|| {
            a.set(&rt, 1);
            a.set(&rt, 2);
            a.set(&rt, 3);
        });
        assert_eq!(runs.get() - before, 1, "batched writes flush once");
    }

    #[test]
    fn update_mutates_in_place() {
        let rt = Runtime::new();
        let v = rt.signal("v", || vec![1, 2, 3]);
        v.update(&rt, |xs| xs.push(4));
        assert_eq!(v.get(&rt), vec![1, 2, 3, 4]);
    }
}
