# Plan — Hash-based reactive identity (`impl Hash + Debug` keys)

*Companion to ADR-021 (decision log §1/§3, 2026-08-02). Refines the identity
clause of ADR-007 ("Identity = call-site path + explicit keys") without changing
the reactivity model. Phased **H0–H4**, each gated on the F0 coherence oracle
(`assert_view_coherent`).*

## Problem

Per-item reactive state costs heap traffic today, and the cost is in the **string
key used to address a signal/scope, not the signal cell itself**:

- **Per new item (one-time):** `Runtime::signal` (`state.rs:520`) →
  `intern` clones the key **twice** (`id_to_key.push(key.to_string())` +
  `key_to_id.insert(key.to_string())`, `state.rs`) plus `Box::new(init())`.
- **Per frame (steady state):** to *address* an item you build a fresh string
  every build — `cx.signal(&format!("todo-{id}"), …)` at the call site, and
  `scoped_key` (`element.rs:710`) does `format!("{p}{name}")` while `scope`
  (`element.rs:615`) does `format!("{key}/")` (`:624`) + `scope_live.insert(
  key.clone())` (`:617`). These allocate even when nothing changed, purely to
  hash back to an interned `SignalId(u32)`.

Reading/writing an existing signal is already allocation-free (`Signal<T>` is a
`Copy` handle; `update` is an in-place `HashMap<SignalId, Slot>` edit,
`state.rs:936`). So the fix is to make **addressing** cheap, not to change the
store.

## Decision (see ADR-021)

`cx.signal` / `cx.scope` (and `Runtime::signal`/`memo`/`effect`) accept
**`impl Hash + Debug`** instead of `&str`. Identity becomes a `u64` folded from
the enclosing scope's hash and the local key's hash (egui's `Id` model). A
readable name is **materialized once, on the cold intern path**, to preserve the
two features that depend on string keys:

- **Snapshot restore** (ADR-011): `StateSnapshot` is keyed by readable name;
  restore matches `pending.remove(&name)` (`state.rs:526`).
- **Agent observability** (ADR-009, Goal 3): `dep_keys` (`state.rs:274`),
  `ui.getDeps`/`whatDependsOn` report *why* a subtree updates, by name.

`&str` implements `Hash + Debug`, so the new bound is a **strict superset** of
today's call sites — existing `cx.signal("text")` / `runtime().signal("hit")`
compile unchanged. Enum variants (`#[derive(Hash, Debug)]`), integers, and tuples
become first-class, allocation-free keys:

```rust
#[derive(Hash, Debug, Clone, Copy)]
enum Field { Filter, Row(u32) }

cx.signal(Field::Row(id), || false)     // Copy key, zero alloc to address
cx.scope(Field::Row(id), |cx| row(cx))  // per-item scope, no format!
cx.signal("filter", || Filter::All)      // &str still works
```

### Identity composition

- Unscoped (root) key: `id_hash = hash(local)`. A top-level `cx.signal("hit")`
  and an external `runtime().signal("hit")` therefore resolve to the **same**
  id — unchanged from today.
- Scoped key: `id_hash = fold(parent_hash, hash(local))` where `fold` is a fixed
  64-bit mix (e.g. FxHash-style rotate+xor+mul; **must be deterministic and
  stable across runs** — no `RandomState`, which would break snapshot restore and
  goldens). The interner maps `u64 → SignalId(u32)` (dense id preserved so
  `Slot`/`ReadSet` versioning is unchanged).

### Readable name (cold path only)

`id_to_key: Vec<String>` (`state.rs:203`) stays, now populated as
`format!("{parent_name}{local:?}")` **only when a brand-new id is interned**. In
steady state every id already exists → the `u64` lookup hits → no name is built.
`parent_name` is an `Rc<str>` built **once per re-running scope** (memoized/
skipped scopes build nothing), shared by all children created within it.

### Collision guard

A `u64` collision would silently alias two signals (and corrupt a snapshot). On
intern, if a computed `u64` already maps to an id whose stored readable name
differs from the one being interned → emit a new stable diagnostic
**`W0xxx` (identity hash collision)** and fall back to linear probing
(`hash+1, hash+2, …`) so identity stays correct. Astronomically rare for a live
UI; the guard makes snapshots safe by construction.

## Phases

### H0 — identity core in `lumen-core` (behind the existing API)
Land the hash-folded interner with **no call-site change** (public fns still take
`&str`; internally route through the new path).

- Replace `key_to_id: HashMap<String, SignalId>` with `hash_to_id:
  HashMap<u64, SignalId>`; keep `id_to_key: Vec<String>` for names.
- `intern_hashed(parent_hash: u64, local_hash: u64, name: impl FnOnce() -> String)
  -> SignalId` — hit path returns without calling `name`; miss path builds the
  name, checks `pending` (snapshot), pushes `id_to_key`, applies the collision
  guard.
- Keep a `fn intern(&self, key: &str)` shim = `intern_hashed(0, hash(key), ||
  key.into())` so existing internal callers are untouched.
- **Tests:** same-key→same-id; distinct-keys→distinct-ids; hash stability across
  process (golden `u64` for a fixed key set); forced-collision path (inject two
  names with equal `u64` via a test seam) emits `W0xxx` and stays correct.
- **Gate:** full `lumen-core` suite green; `assert_view_coherent` unaffected.

### H1 — `BuildCx` threading + `impl Hash + Debug` surface
- `BuildCx`: replace `prefix: RefCell<String>` (`element.rs:562`) with
  `prefix_hash: u64` (Copy) + `prefix_name: RefCell<Option<Rc<str>>>` (lazily
  built on first cold intern within the scope).
- `scope<K: Hash + Debug>(&mut self, id: K, f)` (`element.rs:615`): fold
  `child_hash = fold(self.prefix_hash, hash(id))`; **only build the `format!`
  prefix string when a child actually interns**; `scope_live`/`scope_cache`
  re-keyed by `u64` (the readable name is no longer needed to *find* the cache).
- `signal<K: Hash + Debug>` / `memo` / `effect` on both `BuildCx` (`element.rs:601`)
  and `Runtime` (`state.rs:520/558/576`) take `impl Hash + Debug`.
- Remove per-frame `scoped_key` `format!` from the steady path (it becomes the
  cold-intern name builder).
- **Back-compat:** `&str`/`String`/`&&str` keys keep compiling (all `Hash +
  Debug`). Audit the ~dozen `format!`-keyed call sites in `widgets_*.rs`/tests;
  they still work but are the migration targets in H3.
- **Gate:** whole workspace builds; `caret_scroll`/`scroll_hit_clip`/grid tests +
  `assert_view_coherent` green; no golden changes (identity is internal).

### H2 — snapshot + agent observability parity
- Confirm `StateSnapshot` (`state.rs`) still serializes by readable name and
  `run_headless_restored` round-trips a scoped-signal snapshot (new test:
  snapshot an app with `cx.scope(Field::Row(i))` rows, restore, assert values).
- `dep_keys` (`state.rs:274`) / `ui.getDeps` / `whatDependsOn` return the same
  readable names as before for `&str` keys, and `"{parent}Row(5)"`-style names
  for enum keys (new conformance assertion in `crates/lumen-agent/tests`).
- Register `W0xxx` in `lumen-core/diagnostics.md` (ADR-019 stable codes).
- **Gate:** `snapshot` build + lean build both green; agent introspection tests
  green.

### H3 — measure + wire into keyed lists (F5 groundwork)
- Add a criterion bench: build a 1 000-row list with per-row signal + scope,
  (a) `&str` `format!` keys vs (b) enum keys — report allocations/frame and
  build time. Record the delta in the plan/decision log.
- Migrate a representative list example (and the `writing-widgets` "namespace
  under `name`" pattern) to enum/index keys.
- Expose the composition as the primitive **F5 `For`** will seed each item's
  scope with (`plan-fine-grained-view.md` follow-on): `For(items, |item| item.id,
  view)` uses `item.id: impl Hash` as the per-row scope identity — no per-frame
  string. This plan delivers the identity primitive; `For`'s list-diff/GC is the
  separate F5 task that consumes it.
- **Gate:** bench shows the steady-state per-frame allocation for the list drops
  to ~0 (only new rows allocate).

### H4 — docs (doc-currency rule, AGENT.md)
- ADR-007 identity clause: note the refinement (→ ADR-021).
- `.ai_docs/02-spec-core.md` §4 (signals/identity): document `impl Hash + Debug`
  keys, composition, and the readable-name/snapshot contract.
- `writing-widgets` skill: change "namespace sub-state under `name`
  (`{name}.text`)" guidance to enum/tuple/index keys; keep `&str` as the simple
  default.
- Decision log §3: mark H0–H4 done with commit range + the H3 bench number.

## Non-goals / explicitly out of scope
- **No change to the reactivity model** — still fine-grained signals + scopes
  (ADR-007), not a central `&mut AppState` / lens engine (that would be a
  diff-based core, a separate ADR — see the 2026-08 discussion).
- **No incremental layout** — the taffy full-tree solve stands (decision log
  2026-07-03, F2).
- **List diffing / GC** — that's F5 `For`; this plan only provides the identity
  primitive it needs.

## Risks
- **Hash stability is load-bearing** for snapshot restore + goldens: the fold and
  per-key `Hash` must never depend on `RandomState`/process seed. Guarded by the
  H0 cross-process golden-`u64` test.
- **Readable-name determinism** depends on derived `Debug` being stable; document
  that a custom `Debug` on a key type must be pure/stable (it feeds snapshot
  keys). A `#[derive(Debug)]` enum is safe.
- **Collision on the snapshot path** — mitigated by the H2 guard + `W0xxx`.
