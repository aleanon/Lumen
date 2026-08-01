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

`parent_name` is an `Rc<str>` built **when a scope re-runs**, shared by every
child interned within that run. *(Reviewed 2026-08-02: it cannot be deferred to
"first cold intern inside the scope" — that would require holding a borrow of the
key past the `scope()` call. It doesn't need to be: a cold intern can only happen
while its scope is **running**, so building the name on re-run makes it available
exactly when needed.)* Skipped/memoized scopes run no code, so they build
nothing — the dominant steady-state path is allocation-free. Root-level
`cx.signal(Field::Row(id))` (no enclosing scope) is zero-alloc unconditionally.

### Scope-local eviction replaces prefix matching (**required**, not optional)

`sweep_dead_scopes` (`app.rs:2009`) sheds a vanished keyed-list item's
scope-local signals via `rt.evict_prefix(&format!("{k}/"))`, and `evict_prefix`
(`state.rs:420`) matches `k.starts_with(prefix)`. **Hash folding is one-way — it
destroys prefix enumerability**, so this must be redesigned or a churning list
leaks one slot per removed row (the exact regression this plan exists to avoid).

Replacement: each `Slot` records `owner: u64` (the scope hash it was interned
under, `0` = root), and `Inner` keeps `scope_parent: HashMap<u64, u64>` recorded
on scope entry. `evict_scope(h)` collects `{h} ∪ descendants(h)` from
`scope_parent` (small — scopes, not signals) and drops slots whose `owner` is in
that set. This is *cheaper* than today's per-key `starts_with` scan and keeps
eviction transitive through nested scopes, which the prefix match gave for free.

### Collision guard → superseded by 128-bit identity (H0, implemented)

*Original design:* a `u64` identity plus a name-comparison guard that emitted a
`W0xxx` diagnostic and linear-probed on collision.

**Rejected while implementing H0**, because the guard defeats the plan's own
goal: comparing names on intern means building the name on the **hit** path —
i.e. allocating on exactly the per-frame path this exists to make free. Checking
only in debug builds is worse: debug and release would then probe differently and
*diverge on identity*.

*Shipped instead:* [`IdHash`] is **128 bits** (`u128`), two independent FxHash
lanes accumulated in one pass. Collision probability is ~2⁻¹²⁸ per pair, so no
probe, no diagnostic, and no name comparison is needed — the hit path is a pure
hash-map lookup. Integer widths are additionally tagged (`1u32` and `1u64` hand
`Hash` identical byte streams, so without a tag they would address the same
signal), and byte runs are length-framed and read little-endian so identity is
stable across platforms.

## Phases

### H0 — identity core in `lumen-core` (behind the existing API) — ☑ DONE 2026-08-02
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
- **Hasher:** a hand-rolled FxHash-style `IdHasher` in `lumen-core` — **not**
  `DefaultHasher` (SipHash keys are documented as unstable across Rust releases,
  which would break persisted snapshots/goldens) and **no new dependency**
  (ADR-003). Deterministic, seed-free, version-pinned by our own source.
- `evict_prefix` keeps working unchanged in H0 by scanning `id_to_key` (indexed
  by `SignalId`) instead of the removed `key_to_id` — same semantics, so H0 is a
  pure refactor with zero behavior change. It is *replaced* by `evict_scope` in
  H1, when prefixes stop existing.
- **Tests:** same-key→same-id; distinct-keys→distinct-ids; hash stability
  (golden `u64` for a fixed key set, so a hasher change fails loudly); forced
  collision (test seam) emits `W0xxx` and keeps both signals distinct.
- **Gate:** full `lumen-core` + `lumen-widgets` suites green; `assert_view_coherent`
  unaffected; no golden changes.

### H1 — `BuildCx` threading + `impl Hash + Debug` surface
- `BuildCx`: replace `prefix: RefCell<String>` (`element.rs:562`) with
  `prefix_hash: u64` (Copy) + `prefix_name: Option<Rc<str>>` built **on scope
  re-run** (see the readable-name section — skipped scopes build nothing; a cold
  intern can only occur inside a running scope, so the name is always there when
  needed).
- `scope<K: Hash + Debug>(&mut self, id: K, f)` (`element.rs:615`): fold
  `child_hash = fold(self.prefix_hash, hash(id))`; `scope_live`/`scope_cache`
  re-keyed by `u64` — the **cache-hit path needs no name at all**, so a memoized
  row costs zero allocations.
- `signal<K: Hash + Debug>` / `memo` / `effect` on both `BuildCx` (`element.rs:601`)
  and `Runtime` (`state.rs:520/558/576`) take `impl Hash + Debug`.
- Remove per-frame `scoped_key` `format!` from the steady path (it becomes the
  cold-intern name builder).
- `Element::scope_key: Option<String>` (`element.rs:203`, cloned per scope per
  build at `:634`/`app.rs:2943-2960`) → `Option<u64>` (Copy) — another
  per-scope-per-frame allocation gone. `scope_spans`/`prev_spans` re-key to `u64`.
- **Nested-scope addressing (public API):** `Headless::scope_span(key: &str)`
  (`app.rs:2715`) is used by `tests/scope_spans.rs` with flat `"outer/inner"`
  keys, which no longer equal `fold(h("outer"), h("inner"))`. Replace with a
  path-shaped accessor that folds the same way the build does; update those
  tests. (Public-surface change, covered by ADR-021's pre-1.0 rationale.)
- **`Runtime::evict_prefix` → `evict_scope(u64)`** per the eviction section;
  `Slot.owner` + `scope_parent` land here; `sweep_dead_scopes` (`app.rs:2009`)
  switches over. This is the step that would otherwise leak scope-local state
  for every vanished list row.
- `memo`'s `intern_scope(&format!("memo:{key}"))` (`state.rs:582`) folds a
  constant `MEMO_TAG` into the hash instead of allocating a tagged string.
- **Back-compat:** `&str`/`String`/`&&str` keys keep compiling (all `Hash +
  Debug`). Audit the ~dozen `format!`-keyed call sites in `widgets_*.rs`/tests;
  they still work but are the migration targets in H3.
- **Gate:** whole workspace builds; `scope_spans`/`scope`/`caret_scroll`/
  `scroll_hit_clip`/grid tests + `assert_view_coherent` green; **a new list-churn
  GC test** (add N rows, drop them, assert the slot count returns to baseline)
  proves the eviction replacement; no golden changes (identity is internal).

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
- **Scope-local eviction is the regression-prone step** (H1): hashes can't be
  prefix-matched, so the `Slot.owner`/`scope_parent` replacement must land in the
  *same* change that removes `evict_prefix`, or churning lists leak silently
  (no test fails today — hence the new list-churn GC test in H1's gate).
