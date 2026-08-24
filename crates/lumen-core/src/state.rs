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
use crate::identity::{fold_id, hash_id, key_name, IdHash, ROOT_ID};
#[cfg(feature = "snapshot")]
use serde::de::DeserializeOwned;
#[cfg(feature = "snapshot")]
use serde::Serialize;
use std::any::Any;
use std::cell::RefCell;
// F2.4: the reactive store's own maps use FxHash, not std's SipHash.
//
// `ReadSet::is_current` probes `slots` once per dep per memoized scope on
// EVERY frame — with one scope per row that is one hash of a `u32` key per row
// per frame, and it measured **8.4%** of a 3000-row memoized frame
// (`sip::Hasher::write` 4.79% + `hash_one` 3.65%). These keys are dense
// interned ids the runtime mints itself, so SipHash's DoS resistance buys
// nothing here — the same argument, and the same fix, as R1's shape cache.
//
// Iteration order changes as a result. Two places iterate these maps into
// output, and both were checked (see `fxhash`'s module note on CP3.1, which is
// the trap this can spring):
//
//   * `snapshot()` inserts into a `serde_json::Map`, which is a `BTreeMap`
//     here — serde_json is built without `preserve_order`, no `indexmap` in
//     the lock — so its JSON is sorted by key and hasher-independent.
//   * `adopt_pending_live()` collects restore diagnostics in `slots` order.
//     Nothing asserts their order (every test uses `.any`), and this is in
//     fact an improvement: std's `RandomState` reseeds per process, so that
//     order was already nondeterministic run to run. FxHash has no random
//     seed, so it becomes stable.
use crate::fxhash::{HashMap, HashSet};
use std::collections::HashSet as StdHashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;
use std::rc::Rc;

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

/// SD6a: the message for a same-key-different-type signal collision.
///
/// Signal keys are strings (`cx.signal("count", …)`), so nothing at compile
/// time stops two call sites sharing a key with different value types. When
/// they do, the second site's `downcast` fails and the old message —
/// `"signal type mismatch"` — named neither the key nor either type, so the
/// reader had no way to find the other site.
///
/// Note that same-key-**same**-type sharing is deliberate and supported: it is
/// how widget state works (`{name}.open` across Sheet/Drawer/Popover/Combobox).
/// Only a *type* disagreement is an error, which is why this is a panic on
/// mismatch rather than a warning on reuse.
fn type_mismatch_msg<T: ?Sized>(key: Option<&str>, found: &'static str) -> String {
    let key = key.unwrap_or("<unknown>");
    format!(
        "signal key {key:?} is already stored as `{found}`, but was just \
         accessed as `{}`.\n\
         Signal keys are strings, so two call sites can collide on one key. \
         Either give this signal a distinct key, or make both sites agree on \
         the type. Sharing a key with the SAME type is fine and intentional \
         (that is how widget state like `{{name}}.open` is shared) — only the \
         type disagreement is the bug.",
        std::any::type_name::<T>(),
    )
}

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

/// Folded into a memo's value identity to derive its recompute-scope identity,
/// so the two never collide with a user key.
const MEMO_SCOPE_TAG: &str = "\u{0}lumen.memo";

/// Interned identity of a stored value (signal or memo). `Copy` so [`Signal`]
/// can be a cheap copyable handle.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct SignalId(u32);

/// Identity of a reactive scope (effect or memo recompute).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct ScopeId(u32);

/// SD6b: a signal key with its value type bound at declaration.
///
/// Signal keys are strings, so two call sites can name the same key with
/// different types. SD6a made that failure *loud* — the panic now names the key
/// and both types — but loud-at-runtime is still runtime. A `SignalKey<T>`
/// carries the type in its own type, so the mismatch cannot be written:
///
/// ```
/// # use lumen_core::state::{Runtime, SignalKey};
/// const COUNT: SignalKey<i64> = SignalKey::new("count");
///
/// let rt = Runtime::new();
/// let a = rt.signal_keyed(COUNT, || 0);
/// let b = rt.signal_keyed(COUNT, || 0);   // same key, necessarily same type
/// a.set(&rt, 7);
/// assert_eq!(b.get(&rt), 7);
/// ```
///
/// Declaring it as a `const` is the point: the key and its type are written
/// once, and every use refers to that declaration instead of re-typing a
/// string. A second site cannot disagree about the type without failing to
/// compile.
///
/// This is additive. The `&str` API stays — same-key-same-type sharing across
/// widgets (`{name}.open` on Sheet/Drawer/Popover/Combobox) is a deliberate
/// contract, and typed keys would make that sharing private by construction.
/// `docs/plan-state-keys.md` records why that must not be broken.
pub struct SignalKey<T> {
    key: &'static str,
    _pd: PhantomData<fn() -> T>,
}

impl<T> SignalKey<T> {
    /// Declare a typed key. `const`-callable so it can live in a `const`.
    pub const fn new(key: &'static str) -> SignalKey<T> {
        SignalKey {
            key,
            _pd: PhantomData,
        }
    }

    /// The underlying string, for interop with the untyped API and for
    /// snapshot/agent surfaces that address state by name.
    pub const fn as_str(&self) -> &'static str {
        self.key
    }
}

impl<T> Clone for SignalKey<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for SignalKey<T> {}

impl<T> std::fmt::Debug for SignalKey<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Debug-prints as the bare key so `key_name` and every diagnostic that
        // formats a key produce the same text as the `&str` API. A typed key
        // must not change what a snapshot or an agent sees.
        f.write_str(self.key)
    }
}

impl<T> std::hash::Hash for SignalKey<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash as the bare string, so `SignalKey::<T>::new("k")` and `"k"`
        // address the SAME signal. Typed and untyped access must agree, or
        // migrating a key to a typed one would silently orphan its state.
        self.key.hash(state);
    }
}

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
    /// NOTE: never call `StoredValue` methods via `self.value.method()` —
    /// in the lean build the `impl<T: 'static> StoredValue for T` blanket
    /// covers `Box<dyn StoredValue>` itself, so method resolution picks the
    /// blanket ON THE BOX (autoref beats deref) and `as_any` reports the
    /// Box's `TypeId`, breaking every downcast. Use [`Slot::stored`]/
    /// [`Slot::stored_mut`] (UFCS through the trait object) instead. The
    /// snapshot build was immune only because its serde bounds exclude the
    /// Box; the lean build shipped broken until P.3b's shell test hit it.
    value: Box<dyn StoredValue>,
    subs: HashSet<ScopeId>,
    /// The `write_gen` at this value's last write (0 = never written since
    /// creation). A [`ReadSet`] records per-signal versions so a memoized view
    /// scope can tell whether *its* deps changed — finer than the global
    /// `write_gen`, which only says *something* changed.
    version: u64,
    /// The reactive scope this value was created under ([`ROOT_ID`] if none) —
    /// what [`Runtime::evict_scope`] sheds when that scope disappears (F5 list
    /// GC). Recorded per slot because identity is a hash: unlike the string
    /// keys this replaced, a scope's descendants cannot be found by prefix.
    owner: IdHash,
    /// Concrete type name of `value`, captured at construction where `T` is
    /// still known (SD6a diagnostics only).
    ///
    /// Recorded rather than derived through the trait object: dispatching a
    /// `StoredValue` method on `dyn StoredValue` ties the receiver borrow to
    /// the trait's `'static` supertrait, which the lean build rejects inside
    /// `Signal::update`'s `borrow_mut` scope. Capturing it here sidesteps that
    /// and costs one word on a struct that already holds a `HashSet`.
    type_name: &'static str,
}

impl Slot {
    /// The stored value as `&dyn Any` — dispatched through the trait
    /// object explicitly (see the field note).
    fn stored(&self) -> &dyn Any {
        <dyn StoredValue as StoredValue>::as_any(&*self.value)
    }
    /// The concrete type name of the stored value (SD6a diagnostics).
    fn stored_type_name(&self) -> &'static str {
        self.type_name
    }
    /// Mutable counterpart of [`stored`](Self::stored).
    fn stored_mut(&mut self) -> &mut dyn Any {
        <dyn StoredValue as StoredValue>::as_any_mut(&mut *self.value)
    }
}

struct ScopeData {
    deps: HashSet<SignalId>,
    run: Rc<dyn Fn(&ReadScope)>,
}

#[derive(Default)]
struct Inner {
    slots: HashMap<SignalId, Slot>,
    scopes: HashMap<ScopeId, ScopeData>,

    // Interning: folded key hash -> dense id (ADR-021). The *readable* name is
    // kept alongside in `id_to_key` (indexed by `SignalId`) because snapshots
    // (ADR-011) and agent dep reporting (ADR-009) are name-keyed — but it is
    // only ever built on the cold path, so re-addressing an existing signal
    // never allocates.
    hash_to_id: HashMap<IdHash, SignalId>,
    id_to_key: Vec<String>,
    scope_hash_to_id: HashMap<IdHash, ScopeId>,
    next_scope: u32,
    /// Reactive-scope tree (child hash -> parent hash), recorded as each
    /// `BuildCx::scope` runs. [`Runtime::evict_scope`] walks it so shedding a
    /// scope also sheds the scopes nested inside it — the transitivity the old
    /// string-prefix match gave for free.
    scope_parent: HashMap<IdHash, IdHash>,

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
    /// O4.6: the painted-frame counter stamped onto each log entry, so an agent
    /// can group entries by the pump that produced them.
    log_frame: Rc<std::cell::Cell<u64>>,
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
    /// Stable diagnostic code, for entries that came from a [`Diagnostic`]
    /// (`"W0103"`, …). `None` for free-text causal entries, which have no code
    /// by design.
    ///
    /// Without this a consumer has to prefix-parse the code back out of
    /// `message` — an unenforced convention rather than a contract.
    pub code: Option<&'static str>,
    /// The agent handle (`"nx-<hex>"`) or author id of the node this entry
    /// concerns, for entries that came from a node-anchored [`Diagnostic`].
    ///
    /// `Diagnostic::fmt` never printed the node, so flattening a diagnostic
    /// into a string destroyed the identity that makes a finding actionable
    /// and left the consumer guessing from whatever backtick-quoted text each
    /// check's author happened to embed.
    pub node: Option<String>,
    /// The painted-frame counter at the time this entry was written.
    ///
    /// The cheap correlation primitive: entries sharing a `frame` came from the
    /// same pump, so an agent can group "what happened when I clicked that"
    /// without doing sequence-number arithmetic against a `app.logs` call it
    /// had to remember to make beforehand.
    pub frame: u64,
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
            log_frame: Rc::new(std::cell::Cell::new(0)),
        }
    }

    /// Append a diagnostic log entry (C.2). Callable from handlers and builds
    /// — a side-channel that never feeds rendering, so build purity holds.
    /// Ring-buffered: the oldest entry drops past 1000.
    pub fn log(&self, level: &'static str, message: impl Into<String>) {
        self.log_entry(level, message.into(), None, None);
    }

    /// Append a log entry carrying a diagnostic's structure (O4.6).
    ///
    /// Used by the ambient audit so a finding keeps its `code` and node anchor
    /// on the way into the ring instead of being flattened into prose the
    /// consumer must re-parse.
    pub fn log_diagnostic(&self, level: &'static str, d: &crate::Diagnostic) {
        let node = d
            .handle
            .as_deref()
            .map(str::to_string)
            .or_else(|| d.node.as_ref().map(|n| n.as_str().to_string()));
        self.log_entry(level, d.to_string(), Some(d.code), node);
    }

    /// The frame counter stamped onto subsequent log entries. Set by the
    /// runtime host each painted frame; `0` when nothing sets it (headless
    /// tests that never pump).
    pub fn set_log_frame(&self, frame: u64) {
        self.log_frame.set(frame);
    }

    fn log_entry(
        &self,
        level: &'static str,
        message: String,
        code: Option<&'static str>,
        node: Option<String>,
    ) {
        let mut l = self.logs.borrow_mut();
        let seq = l.0;
        l.0 += 1;
        if l.1.len() >= 1000 {
            l.1.pop_front();
        }
        l.1.push_back(LogEntry {
            seq,
            level,
            message,
            code,
            node,
            frame: self.log_frame.get(),
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

    /// Drop every stored signal owned by `scope` **or by a scope nested inside
    /// it** (F5 list GC): a keyed scope that vanished this build sheds its
    /// scope-local state, so a churning list doesn't leak slots. The interned
    /// key↔id mapping is kept (cheap), so re-adding the same key re-creates the
    /// slot from its initializer. Returns how many slots were removed.
    pub fn evict_scope(&self, scope: IdHash) -> usize {
        // Everything nested under `scope` goes too.
        let dead = self.subtree_scopes(scope);
        let mut b = self.inner.borrow_mut();

        let ids: Vec<SignalId> = b
            .slots
            .iter()
            .filter(|(_, slot)| dead.contains(&slot.owner))
            .map(|(id, _)| *id)
            .collect();
        let mut n = 0;
        for id in ids {
            if b.slots.remove(&id).is_some() {
                n += 1;
            }
        }
        b.scope_parent.retain(|child, _| !dead.contains(child));
        n
    }

    /// `scope` plus every scope nested inside it, transitively.
    ///
    /// Identity is a hash, so descendants can't be found by prefix the way
    /// string keys allowed — they're walked from the recorded scope tree
    /// instead. Iterating to a fixed point over `scope_parent` (scopes, not
    /// signals — a small map) keeps this transitive regardless of nesting depth
    /// or insertion order.
    pub fn subtree_scopes(&self, scope: IdHash) -> StdHashSet<IdHash> {
        let b = self.inner.borrow();
        let mut set: StdHashSet<IdHash> = StdHashSet::new();
        set.insert(scope);
        loop {
            let mut grew = false;
            for (child, parent) in b.scope_parent.iter() {
                if set.contains(parent) && set.insert(*child) {
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        set
    }

    /// The scope `child` was recorded as nested directly inside, if any.
    ///
    /// Walking *up* is O(depth) where [`subtree_scopes`](Self::subtree_scopes)
    /// is O(scopes²) — the F5 sweep uses this to test one candidate cheaply.
    pub fn parent_scope(&self, child: IdHash) -> Option<IdHash> {
        self.inner.borrow().scope_parent.get(&child).copied()
    }

    /// Record that reactive scope `child` is nested directly inside `parent`.
    ///
    /// Called as each `BuildCx::scope` runs; it is what lets
    /// [`Runtime::evict_scope`] shed nested scopes transitively.
    #[doc(hidden)]
    pub fn note_scope(&self, child: IdHash, parent: IdHash) {
        self.inner.borrow_mut().scope_parent.insert(child, parent);
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
        let mut seen = HashSet::default();
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

    /// Create or re-attach a signal keyed by `key` (02 §4); on restore, a staged
    /// snapshot value is adopted instead of `init`.
    ///
    /// `key` is anything `Hash + Debug` (ADR-021) — a `&str`, an index, or a
    /// typed key like `Field::Row(id)`. A typed key costs no allocation to
    /// re-address, which is what makes per-item state cheap in a list:
    ///
    /// ```
    /// # use lumen_core::state::Runtime;
    /// #[derive(Hash, Debug)]
    /// enum Field { Filter, Row(u32) }
    /// let rt = Runtime::new();
    /// let filter = rt.signal(Field::Filter, String::new);
    /// let row_3 = rt.signal(Field::Row(3), || false);
    /// let by_name = rt.signal("legacy-key", || 0i32);   // `&str` still works
    /// # let _ = (filter, row_3, by_name);
    /// ```
    ///
    /// `T` is first in the generic list so an explicit `signal::<MyState>(key, …)`
    /// keeps working.
    pub fn signal<T: State, K: Hash + Debug>(&self, key: K, init: impl FnOnce() -> T) -> Signal<T> {
        self.signal_at(
            fold_id(ROOT_ID, hash_id(&key)),
            ROOT_ID,
            || key_name(&key),
            init,
        )
    }

    /// SD6b: create or re-attach state through a [`SignalKey<T>`], so the key
    /// and its value type are declared together and cannot disagree.
    ///
    /// Identical addressing to [`signal`](Self::signal) with the same string —
    /// `SignalKey` hashes as its bare key — so a codebase can migrate one call
    /// site at a time without orphaning the state.
    pub fn signal_keyed<T: State>(&self, key: SignalKey<T>, init: impl FnOnce() -> T) -> Signal<T> {
        self.signal(key, init)
    }

    /// [`Runtime::signal`] at an already-folded identity, owned by scope
    /// `owner`. The plumbing `BuildCx` uses to thread its enclosing scope; `name`
    /// is invoked only if this key has never been seen.
    // Not the `entry` pattern: the slot value is built from `init`/`pending`,
    // and `b` is borrowed for the pending map in between.
    #[allow(clippy::map_entry)]
    #[doc(hidden)]
    pub fn signal_at<T: State>(
        &self,
        hash: IdHash,
        owner: IdHash,
        name: impl FnOnce() -> String,
        init: impl FnOnce() -> T,
    ) -> Signal<T> {
        let id = self.intern_hashed(hash, name);
        let mut b = self.inner.borrow_mut();
        if !b.slots.contains_key(&id) {
            // On restore, adopt a staged snapshot value instead of `init`.
            // Snapshots are keyed by the readable name (ADR-011), which the
            // intern above has recorded by now.
            #[cfg(feature = "snapshot")]
            let value: Box<dyn StoredValue> = {
                let key = b.id_to_key[id.0 as usize].clone();
                match b.pending.remove(&key) {
                    Some(json) => match deser_lenient::<T>(&key, &json) {
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
                }
            };
            #[cfg(not(feature = "snapshot"))]
            let value: Box<dyn StoredValue> = Box::new(init());
            b.slots.insert(
                id,
                Slot {
                    value,
                    subs: HashSet::default(),
                    version: 0,
                    owner,
                    type_name: std::any::type_name::<T>(),
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
    ///
    /// `key` is anything `Hash` (ADR-021) — see [`Runtime::signal`].
    pub fn effect<K: Hash>(&self, key: K, f: impl Fn(&ReadScope) + 'static) {
        self.effect_at(fold_id(ROOT_ID, hash_id(&key)), f)
    }

    /// [`Runtime::effect`] at an already-folded identity (the `BuildCx` seam).
    #[doc(hidden)]
    pub fn effect_at(&self, hash: IdHash, f: impl Fn(&ReadScope) + 'static) {
        let id = self.intern_scope_hashed(hash);
        {
            let mut b = self.inner.borrow_mut();
            b.scopes.insert(
                id,
                ScopeData {
                    deps: HashSet::default(),
                    run: Rc::new(f),
                },
            );
        }
        self.run_scope(id);
    }

    /// Create or re-attach a memo: a derived value recomputed when its
    /// dependencies change, notifying *its* subscribers only when the value
    /// actually changes (`PartialEq`).
    ///
    /// `key` is anything `Hash + Debug` (ADR-021) — see [`Runtime::signal`].
    pub fn memo<T: PartialEq + State, K: Hash + Debug>(
        &self,
        key: K,
        f: impl Fn(&ReadScope) -> T + 'static,
    ) -> Memo<T> {
        self.memo_at(
            fold_id(ROOT_ID, hash_id(&key)),
            ROOT_ID,
            || key_name(&key),
            f,
        )
    }

    /// [`Runtime::memo`] at an already-folded identity, owned by `owner` (the
    /// `BuildCx` seam).
    #[doc(hidden)]
    pub fn memo_at<T: PartialEq + State>(
        &self,
        hash: IdHash,
        owner: IdHash,
        name: impl FnOnce() -> String,
        f: impl Fn(&ReadScope) -> T + 'static,
    ) -> Memo<T> {
        let value_id = self.intern_hashed(hash, name);
        // The recompute scope is a sibling identity of the value, not a string
        // tag spliced onto the key.
        let scope_id = self.intern_scope_hashed(fold_id(hash, hash_id(&MEMO_SCOPE_TAG)));
        let rt = self.clone();
        let run = move |scope: &ReadScope| {
            let v = f(scope);
            rt.update_memo_value::<T>(value_id, owner, v);
        };
        {
            let mut b = self.inner.borrow_mut();
            b.scopes.insert(
                scope_id,
                ScopeData {
                    deps: HashSet::default(),
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

    /// Run `f` with writes batched: subscribed scopes flush once, after `f`
    /// returns, instead of after each write.
    ///
    /// Unwind-safe: if `f` panics the depth is still restored. That matters
    /// because handler and rebuild panics are *caught* upstream (the subtree
    /// error boundary and `rebuild`'s `catch_unwind`), so a leaked depth would
    /// leave [`flush`](Self::flush) permanently short-circuited — it returns
    /// early while `batch_depth > 0` — and the app would silently stop
    /// reacting to every later write, with no panic and no diagnostic.
    pub fn batch<R>(&self, f: impl FnOnce() -> R) -> R {
        /// Restores the batch depth on the way out, panic or not.
        struct DepthGuard<'a>(&'a Runtime);
        impl Drop for DepthGuard<'_> {
            fn drop(&mut self) {
                self.0.inner.borrow_mut().batch_depth -= 1;
            }
        }

        self.inner.borrow_mut().batch_depth += 1;
        let r = {
            let _depth = DepthGuard(self);
            f()
        };
        // Deliberately *outside* the guard: `flush` runs subscribed scopes,
        // which need the store the guard would still be unwinding through, and
        // a panic escaping a destructor during unwind aborts the process. A
        // panicking batch therefore restores the depth and skips the flush;
        // the next successful write flushes the accumulated dirty set.
        if self.inner.borrow().batch_depth == 0 {
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
            map.insert(key, <dyn StoredValue as StoredValue>::to_json(&*slot.value));
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
                match <dyn StoredValue as StoredValue>::restore_json(&mut *slot.value, key, &json) {
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

    /// Resolve a folded key hash to its dense [`SignalId`], creating the mapping
    /// on first sight (ADR-021).
    ///
    /// `name` is called **only on the cold path** — when this hash has never
    /// been seen. Re-addressing an existing signal is a hash-map hit with no
    /// allocation, which is what lets a per-item key be rebuilt every frame for
    /// free. The name it returns is the readable key snapshots and agent dep
    /// reporting use.
    fn intern_hashed(&self, hash: IdHash, name: impl FnOnce() -> String) -> SignalId {
        let mut b = self.inner.borrow_mut();
        if let Some(&id) = b.hash_to_id.get(&hash) {
            return id;
        }
        let id = SignalId(b.id_to_key.len() as u32);
        b.id_to_key.push(name());
        b.hash_to_id.insert(hash, id);
        id
    }

    /// Resolve a folded key hash to its dense [`ScopeId`] (effects/memos).
    /// Scope ids are never reported by name, so unlike [`Runtime::intern_hashed`]
    /// this keeps no readable key at all.
    fn intern_scope_hashed(&self, hash: IdHash) -> ScopeId {
        let mut b = self.inner.borrow_mut();
        if let Some(&id) = b.scope_hash_to_id.get(&hash) {
            return id;
        }
        let id = ScopeId(b.next_scope);
        b.next_scope += 1;
        b.scope_hash_to_id.insert(hash, id);
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
        let v = slot.stored().downcast_ref::<T>().unwrap_or_else(|| {
            panic!(
                "{}",
                type_mismatch_msg::<T>(
                    b.id_to_key.get(id.0 as usize).map(String::as_str),
                    slot.stored_type_name(),
                )
            )
        });
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

    fn update_memo_value<T: PartialEq + State>(&self, id: SignalId, owner: IdHash, value: T) {
        // Memo recompute runs mid-flush: enqueue dependents but never flush here.
        let mut b = self.inner.borrow_mut();
        let changed = match b.slots.get(&id) {
            Some(slot) => slot
                .stored()
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
                        subs: HashSet::default(),
                        version: ver,
                        owner,
                        type_name: std::any::type_name::<T>(),
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
                let key = b.id_to_key.get(self.id.0 as usize).cloned();
                let slot = b.slots.get_mut(&self.id).expect("signal slot missing");
                slot.version = ver;
                let found = slot.stored_type_name();
                let v = slot
                    .stored_mut()
                    .downcast_mut::<T>()
                    .unwrap_or_else(|| panic!("{}", type_mismatch_msg::<T>(key.as_deref(), found)));
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
    use crate::identity::ScopePath;
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
        let sigs: Vec<Signal<i32>> = (0..N).map(|i| rt.signal(format!("s{i}"), || 0)).collect();
        let counter = Rc::new(Cell::new(0u64));
        for (i, &s) in sigs.iter().enumerate() {
            let c = counter.clone();
            rt.effect(format!("e{i}"), move |scope| {
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
    fn a_panicking_batch_does_not_leak_the_depth() {
        // Regression: `batch` used to decrement `batch_depth` only on the
        // normal path. Handler/rebuild panics are caught upstream, so an
        // unwinding batch left the depth pinned above zero, `flush` returned
        // early forever, and the app stopped reacting — silently, no panic.
        let rt = Runtime::new();
        let sig = rt.signal("v", || 0i32);
        let runs = Rc::new(Cell::new(0u64));
        {
            let c = runs.clone();
            rt.effect("e", move |scope| {
                let _ = sig.get(scope);
                c.set(c.get() + 1);
            });
        }
        assert_eq!(runs.get(), 1, "effect runs once on registration");

        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rt.batch(|| {
                sig.set(&rt, 1);
                panic!("handler blew up mid-batch");
            })
        }));
        assert!(caught.is_err(), "the panic must still propagate");

        // The store is still live: a later write flushes normally.
        sig.set(&rt, 2);
        assert!(
            runs.get() > 1,
            "runtime stopped reacting after a panicking batch (leaked batch_depth)"
        );
        assert_eq!(sig.get(&rt), 2);
    }

    #[test]
    fn nested_batches_still_flush_once_at_the_outermost_exit() {
        let rt = Runtime::new();
        let sig = rt.signal("v", || 0i32);
        let runs = Rc::new(Cell::new(0u64));
        {
            let c = runs.clone();
            rt.effect("e", move |scope| {
                let _ = sig.get(scope);
                c.set(c.get() + 1);
            });
        }
        let before = runs.get();
        rt.batch(|| {
            sig.set(&rt, 1);
            rt.batch(|| sig.set(&rt, 2));
            // The inner batch must not have flushed.
            assert_eq!(runs.get(), before, "inner batch flushed early");
        });
        assert_eq!(
            runs.get(),
            before + 1,
            "outer batch must flush exactly once"
        );
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
            rt.signal(format!("k{i}"), || i * 3);
        }
        let snap = rt.snapshot();

        let rt2 = Runtime::new();
        rt2.load_pending(snap);
        let restored: Vec<Signal<i64>> = (0..N)
            .map(|i| rt2.signal(format!("k{i}"), || -1)) // init must be ignored
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

    // --- identity / interning (ADR-021) ------------------------------------

    /// The point of hash identity: **re-addressing an existing signal must not
    /// build its readable name.** That name is the only allocation left on the
    /// path, so if it were built per call, a per-item key rebuilt every frame
    /// (a list row) would allocate every frame — the cost this replaces.
    #[test]
    fn a_readable_name_is_built_only_when_the_id_is_new() {
        let rt = Runtime::new();
        let built = Cell::new(0);
        let h = fold_id(ROOT_ID, hash_id("row-7"));

        let first = rt.intern_hashed(h, || {
            built.set(built.get() + 1);
            "row-7".to_string()
        });
        assert_eq!(built.get(), 1, "a brand-new id must record its name");

        for _ in 0..100 {
            let again = rt.intern_hashed(h, || {
                built.set(built.get() + 1);
                "row-7".to_string()
            });
            assert_eq!(again, first, "the same key must resolve to the same id");
        }
        assert_eq!(
            built.get(),
            1,
            "re-addressing an existing signal must not build its name"
        );
    }

    #[test]
    fn distinct_keys_get_distinct_slots() {
        let rt = Runtime::new();
        let a = rt.signal("a", || 1i32);
        let b = rt.signal("b", || 2i32);
        a.set(&rt, 10);
        assert_eq!(a.get(&rt), 10);
        assert_eq!(b.get(&rt), 2, "writing `a` must not touch `b`");
    }

    #[test]
    fn re_creating_the_same_key_keeps_the_existing_value() {
        let rt = Runtime::new();
        let a = rt.signal("a", || 1i32);
        a.set(&rt, 42);
        // The initializer must not run again for an existing key.
        let again: Signal<i32> = rt.signal("a", || 1i32);
        assert_eq!(again.get(&rt), 42);
    }

    /// `evict_scope` is the F5 list GC (a vanished keyed-list row sheds its
    /// scope-local state). Under string keys this was a prefix match; identity
    /// is a hash now, so it walks recorded slot ownership instead. Untested
    /// before H0 — which is why the hazard was invisible.
    #[test]
    fn evict_scope_drops_only_the_signals_that_scope_owns() {
        let rt = Runtime::new();
        let row1 = ScopePath::root().child("row-1").hash();
        let row2 = ScopePath::root().child("row-2").hash();

        let keep = rt.signal("keep", || 1i32);
        let _r1 = rt.signal_at(
            fold_id(row1, hash_id("count")),
            row1,
            || "c".into(),
            || 10i32,
        );
        let r2 = rt.signal_at(
            fold_id(row2, hash_id("count")),
            row2,
            || "c".into(),
            || 20i32,
        );

        assert_eq!(rt.evict_scope(row1), 1, "exactly the one row is shed");

        // The evicted slot is gone, so re-creating the key runs its initializer
        // afresh — while its neighbours are untouched.
        let again: Signal<i32> = rt.signal_at(
            fold_id(row1, hash_id("count")),
            row1,
            || "c".into(),
            || 99i32,
        );
        assert_eq!(again.get(&rt), 99);
        assert_eq!(r2.get(&rt), 20, "a sibling scope keeps its state");
        assert_eq!(keep.get(&rt), 1, "root-level state is untouched");
    }

    /// Prefix matching used to make eviction transitive for free: dropping
    /// `row-1/` also dropped `row-1/inner/`. Hashes can't be prefix-matched, so
    /// the scope tree has to reproduce it — a nested scope's state must not
    /// survive its parent.
    #[test]
    fn evict_scope_is_transitive_through_nested_scopes() {
        let rt = Runtime::new();
        let row = ScopePath::root().child("row-1").hash();
        let inner = ScopePath::root().child("row-1").child("editor").hash();
        let deep = ScopePath::root()
            .child("row-1")
            .child("editor")
            .child("undo")
            .hash();
        rt.note_scope(inner, row);
        rt.note_scope(deep, inner);

        rt.signal_at(fold_id(row, hash_id("a")), row, || "a".into(), || 1i32);
        rt.signal_at(fold_id(inner, hash_id("b")), inner, || "b".into(), || 2i32);
        rt.signal_at(fold_id(deep, hash_id("c")), deep, || "c".into(), || 3i32);
        let survivor = rt.signal("outside", || 4i32);

        assert_eq!(
            rt.evict_scope(row),
            3,
            "the scope and everything nested inside it are shed"
        );
        assert_eq!(survivor.get(&rt), 4);
        assert_eq!(rt.len(), 1, "only the unrelated signal remains");
    }
}
