# Plan: `#[derive(Reactive)]` — Xilem-shaped authoring over the existing store (RD-series)

> # ⛔ RETIRED 2026-08-04 — DO NOT IMPLEMENT. Superseded by `docs/plan-state-keys.md`.
>
> Retired the same day it was written, before any phase started, after three
> independent reviews (architecture, Rust feasibility, migration). Nothing below
> was built. **Read this box before re-proposing anything in this shape** — the
> plan is kept, not deleted, precisely because a superseded task buried in an
> unread document is what produced the N3.4 mistake in `plan-node-cost.md`.
>
> **The governance reason, which is on its own sufficient.** The ADR-021 design
> entry (`.ai_docs/07-decision-log.md`, 2026-08-02 — *two days before this plan*)
> already ruled on it: *"**Not a reactivity-model change** — fine-grained signals
> + scopes stand (**NOT a central `&mut AppState`/lens diff-engine, which would be
> a separate ADR**)."* This plan is that thing, and its ADR-impact table claimed
> ADR-007 "unchanged" with no new-ADR row. ADR-007 is not unchanged at the wording
> level either: it reads *"Identity = call-site path + explicit keys"*, whereas
> derive identity is a **type field path**, call-site independent.
>
> **The three structural faults**, each verified against the code:
>
> 1. **Derive state can only ever be root-owned.** `BuildCx::signal`
>    (`element.rs:686`) folds into `self.prefix_hash` — the enclosing `cx.scope` —
>    while `Runtime::signal` (`state.rs:588`) folds into `ROOT_ID`, and `Runtime`
>    has no ambient scope. A `Read<T>`/`Write<T>` holding only a `&Runtime`
>    therefore cannot address scope-local state at all. Measured:
>    `fold_id(fold_id(fold_id(ROOT,"todos"),47),"done")` = `0x27aa88f1…` vs
>    `fold_id(ROOT,"todos[47].done")` = `0x44d663f9…`. The foundational
>    "same `IdHash`" invariant holds only for flat top-level state. The same
>    decision-log entry names this exact hazard: *"flat-string addressing of
>    scoped state … silently turned `benches/perf.rs`'s `scope_memo_one_of_many`
>    into a no-op-rebuild measurement."*
> 2. **The benefit doesn't survive contact with the widget set.** Widget-owned
>    state is string-keyed *public API* — `{name}.open`, `{name}.selected`,
>    `{name}.page`, `{name}.path` across at least 13 widget files
>    (`sheet.rs:30`, `combobox.rs:52`, `pick_list.rs:76`, …), documented in
>    `building-apps`. A derive app that opens a Sheet still writes
>    `cx.signal("cart.open", …)`. The key discipline the plan existed to kill is
>    not eliminable from the app layer, so every non-trivial derive app is a
>    hybrid, and the two styles are not peers — the derive is a strictly weaker
>    root-only subset.
> 3. **F5 list GC would leak every derive slot.** `evict_scope` (`state.rs:441`)
>    reclaims by `Slot.owner` walked through `scope_parent`; root-owned derive
>    slots are never swept, so `todos[47].*` survives `todos.remove(47)` forever.
>    Fixing it requires a `lumen-core` change, contradicting the plan's own
>    "nothing in `lumen-core` changes semantics" premise. Note the precedent in
>    the same log entry: `evict_prefix` "had to die" because it *"would have leaked
>    one slot per removed list row, **silently, with every existing test green**."*
>
> **Also found, and worth keeping:**
> - The plan's stated top risk (RD1.1, a const-eval `hash_str` matching `hash_id`
>   bit-for-bit) is **not a risk** — verified on rustc 1.94.0 across a 19-string
>   corpus and 1 000 random fold pairs, 0 mismatches. The risk ranking was
>   inverted; RD3.5's list GC was the real one and wasn't on the table.
> - RD3.3's flagship hazard **cannot fire**: `*s.a_mut() = s.b()` evaluates the
>   value operand before the place expression, so no guard is live. The forms that
>   do violate purity already panic through `RefCell`, in release too.
> - RD3.4 would have routed every handler through `Runtime::batch`, which leaked
>   `batch_depth` on unwind. **That bug was real and is now fixed** (`state.rs`,
>   `a_panicking_batch_does_not_leak_the_depth`) — the one durable good this plan
>   produced.
> - The plan's flagship snippet did not compile: `text!` parses exactly
>   `(Expr, LitStr)` with bare-identifier holes (`lumen-macros/src/lib.rs:96–124`).
> - `stable_handler!` hard-codes `Copy + Fn(&Runtime)` (`lumen-macros/src/lib.rs:47`),
>   so it rejects derive-shaped handlers outright — a macro change, not doctests.
>
> **If the Xilem authoring model is still wanted**, it needs its own ADR and an
> honest justification as an *authoring-model change* — not as key-discipline
> relief, which it does not deliver (fault 2) and which has cheaper fixes
> (`plan-state-keys.md`).

*Design + build plan, 2026-08-04. Companion to `plan-fine-grained-view.md`
(the F-series store this sits on top of) and `plan-hash-identity.md` (ADR-021,
whose identity primitive this reuses).*

> **Origin.** A 2026-08 design discussion asked whether Lumen could adopt
> Xilem-style authoring — one owned state struct, callbacks that take `&mut` —
> while keeping O(changed) updates, and whether macros could generate the
> reactivity at compile time instead of tracking it at runtime.
>
> The conclusion was **yes to the authoring model, no to fully static tracking**.
> The tractable design generates *identity* at compile time and leaves *read
> collection* dynamic. This plan builds that.

---

## Thesis

Lumen's reactivity is not expensive. `idle_frame` is 42.9 ns and
`Signal::update` is 16 ns; read collection is a version-comparison, not a
subscription graph. There is no performance problem to solve here.

What *is* expensive is the **authoring ceremony**: `cx.signal("draft", || 0)`
requires the author to invent and maintain a stable key for every piece of state.
Key discipline is a documented recurring trap — it appears as a hazard in the
`building-apps`, `writing-widgets`, and `verifying-apps` skills. A wrong or
duplicated key produces state that silently aliases or resets.

So the goal is ergonomic, not numeric:

```rust
// today
let count = cx.signal("count", || 0i32);
let v = count.get(cx.runtime());
button("+").on_click(move |rt| count.update(rt, |c| *c += 1));

// with the derive
#[derive(Reactive, Serialize, Deserialize)]
struct App { count: i32, todos: Vec<Todo> }

fn body(s: &Read<App>) -> Element {
    text!(cx, "{}", s.count())
}
button("+").on_click(|s: &mut Write<App>| *s.count_mut() += 1)
```

The state struct is the source of truth for *names*; the compiler derives the
keys. No string literals, no aliasing, no drift.

---

## Foundational invariant (do not violate)

> **`#[derive(Reactive)]` is a pure front end.** A program written with the derive
> and the identical program written with `cx.signal` must produce **the same
> store**: the same slots, the same `IdHash` identities, the same readable key
> names, the same `ReadSet`s, the same snapshot JSON, and the same agent dep keys.

Nothing in `lumen-core` changes semantics. This is what makes the plan additive
and low-risk:

- the 385 existing `cx.signal` call sites keep working, untouched;
- F1 scope memoization, F3 `Dynamic`/`Prop` bindings, F5 keyed lists, and the F4
  agent verbs (`getDeps` / `whatDependsOn` / `lastChange`) all work on derive-based
  state **for free**, because underneath it is the same store;
- ADR-007 is unchanged — this is still fine-grained signals;
- the F0 coherence oracle applies unmodified.

If a phase below cannot preserve this invariant, that phase does not ship.

---

## What this is explicitly **not**

Two alternative designs were considered and rejected. Recording why, so they are
not re-proposed:

**Rejected — static field masks (fully compile-time reactivity).** A `#[view]`
macro extracting read sets from expression bodies, unioned in const context via
associated `const MASK: u64`. Composition across annotated functions is genuinely
feasible in Rust. It fails on three counts: (1) granularity collapses to the field,
so editing one row of `todos: Vec<Todo>` dirties every view reading `todos` —
throwing away exactly what ADR-021 and F5 keyed lists were built to deliver
(1 000 per-row signals: 51.0 µs/1 000 allocations → 18.2 µs/**0**); (2) every
`dyn` boundary erases the mask to "reads everything", and a widget library is made
of `dyn` boundaries; (3) the industry precedent is negative — Svelte 3/4 shipped
compile-time assignment analysis and retreated to runtime signals in Svelte 5.

**Rejected — post-hoc structural diff.** Hand the callback a raw `&mut AppState`
and derive the dirty set by diffing before/after. Exact and annotation-free, but
O(state) per event instead of O(1), scaling with the wrong quantity.

**Also not this plan:** deprecating `cx.signal`. Both styles coexist. Whether the
derive becomes the *recommended* default is a separate decision, deferred to RD5.4
and gated on RD5.1's data.

---

# Phase RD0 — Hand-written spike + the equivalence oracle *(do this first)*

Write no proc macro until the shape is proven by hand. RD0 exists so that RD1–RD4
are transcription, not discovery.

## Steps (each independently green)
- **RD0.1 — Hand-write the expansion.** For a three-field struct including one
  `Vec<T>`, write out by hand exactly what the derive would emit: the const path
  identities, the read accessors, the write guards. It must compile and behave.
- **RD0.2 — The equivalence oracle.** `assert_store_eq(rt_a, rt_b)`: same slot set
  keyed by readable name, same values, same versions, same snapshot JSON, same
  `dep_keys`. This is the test every later phase is measured against, and it is
  the executable form of the foundational invariant.
- **RD0.3 — Choose the read spelling with data.** Accessor methods (`s.count()`)
  vs a `Deref` proxy. Microbenchmark both against a direct `Signal::get`; pick on
  measured overhead and on which produces better rustc error messages. Record the
  choice here.

*Acceptance:* the hand-written prototype passes `assert_store_eq` against the
`cx.signal` version of the same app; the read spelling is chosen with numbers.

---

# Phase RD1 — Compile-time field-path identity

## The one real engineering risk, and why it is tractable

The derive must produce, at compile time, the **same** `IdHash` that
`hash_id(&"count")` produces at runtime — otherwise snapshot keys (ADR-011) and
agent dep names (ADR-009) diverge from the existing scheme, breaking the
foundational invariant.

`hash_id<K: Hash>` routes through `std::hash::Hash`, which is not const-callable.
But the hasher is first-party and its arithmetic is const-compatible:

- `IdHasher::new()` is **already `const fn`** (`identity.rs:57`).
- `mix` / `mix_int` are wrapping multiply, xor, and `rotate_left` — all const.
- `finish128` is two shifts — const.
- `write` uses `chunks_exact` + `try_into`, which are not const, but the same
  bytes can be walked with an indexed loop (const loops are stable well below the
  pinned MSRV of 1.94.0), and `u64::from_le_bytes` is const.
- `<str as Hash>::hash` is defined as `write(self.as_bytes())` followed by
  `write_u8(0xff)`, and this impl overrides `write_u8` to `mix_int(i, 1)`.

So a `const fn hash_str(&str) -> IdHash` reproducing that exact sequence is
writable. The proof obligation is a test, not a hope.

## Steps (each independently green)
- **RD1.1 — `const fn hash_str` + `const fn fold_id`.** Add both to
  `lumen-core::identity`. `fold_id` is already pure wrapping arithmetic; making it
  `const` is mechanical. **Gate:** a test asserting `hash_str(s) == hash_id(s)`
  over a corpus (empty, 1–16 bytes, multibyte UTF-8, embedded NUL, ≥64 bytes) and
  `const_fold_id(a,b) == fold_id(a,b)` over random pairs. Endianness is already
  pinned explicitly in `write`; keep it that way.
  *Fallback if const-eval proves impractical:* a `OnceCell`-cached lazy fold per
  field — still allocation-free after first use, just not literally const. The
  invariant survives either way; only the cold-path cost differs.
- **RD1.2 — Derive emits per-field constants.** `#[derive(Reactive)]` emits, for
  each field, `const PATH: &str = "count"` and
  `const ID: IdHash = const_fold_id(PARENT, hash_str("count"))`.
- **RD1.3 — Nested structs.** A field whose type also derives `Reactive` folds its
  own path under the parent's — `fold_id(parent_id, hash_str("done"))` for
  `todos[i].done`.
- **RD1.4 — Readable names.** Emit `"count"`, `"todos"`, `"todos[47].done"`.
  `key_name` strips the quotes `Debug` adds to string keys precisely so
  pre-ADR-021 snapshots keep matching; the derive's names must land on the same
  side of that rule.

*Acceptance:* derive-generated identity is bit-identical to
`cx.signal("count", …)` for the flat case; snapshot JSON keys round-trip; the
`hash_str` corpus test passes.

---

# Phase RD2 — Read side: tracked accessors

## Steps (each independently green)
- **RD2.1 — Per-field read accessor.** Generate
  `fn count(&self, cx: &impl ReadCx) -> i32`, implemented as
  `Runtime::signal_at(Self::COUNT_ID, owner, name, init).get(cx)`. Reads flow
  through the existing collector stack, so `ReadSet`, `collect_reads`, the F1 memo,
  F3 bindings, and `getDeps` all keep working with no changes.
- **RD2.2 — `Read<T>` wrapper.** Holds the `&Runtime` so the call site is `s.count()`
  with no `cx` threading. Implements `ReadCx` passthrough.
- **RD2.3 — Collections keep per-element granularity.** `s.todos().at(i)` yields a
  handle at path `todos[i]`, backed by `signal_at` with a folded index key. This is
  the phase where the design earns its keep versus static masks — and where
  ADR-021's allocation-free re-addressing pays off directly.
- **RD2.4 — Run-count tests.** Mirroring F1's: prove that a write to
  `todos[47].done` re-runs only the scope that read it, and that scopes reading
  `todos[3]` are untouched.

*Acceptance:* run-count tests pass; `assert_store_eq` holds against the equivalent
`cx.signal` program; a `cx.scope`-per-row list re-runs exactly one row.

---

# Phase RD3 — Write side: the mutation guard

This is the core of the design — where "callbacks on mutable state" becomes real.

## Steps (each independently green)
- **RD3.1 — `Write<T>` + `FieldGuard`.** Generate `count_mut()` returning a
  `FieldGuard<'_, i32>` that derefs to the value and, on `Drop`, bumps that slot's
  version. Semantically identical to `Signal::update`, spelled as field navigation.
- **RD3.2 — Nested and indexed writes.** `s.todos_mut().at_mut(47).done_mut()`
  bumps only `todos[47].done`. Parent paths are **not** bumped — that is the whole
  point — but see RD3.5.
- **RD3.3 — Re-entry guard (hazard).** `Signal::update` closures must stay pure: no
  runtime re-entry. A `DerefMut` guard makes violating this syntactically easy —
  `*s.a_mut() = s.b()` reads the store while a guard is live. Add a re-entry flag on
  `Runtime` that panics in debug builds with a diagnostic naming both paths. This is
  a real regression risk introduced by the ergonomics, and it must be caught at the
  call site, not in a golden.
- **RD3.4 — Automatic batching.** A handler receiving `Write<T>` runs inside
  `Runtime::batch`, so N field writes produce N slot-version bumps but **one**
  write-gen bump. Strictly better than today, where each `set` bumps write-gen
  independently.
- **RD3.5 — Structural writes.** `s.todos_mut().push(t)` changes the *shape* of the
  collection, not one element. Define and test the invalidation rule: a length
  change bumps the collection's own path (so keyed-list scopes re-run and F5's
  mark-and-sweep GC reclaims dropped per-row slots via `evict_scope`), while an
  in-place element edit does not.

*Acceptance:* a handler mutating three fields yields exactly three slot bumps and
one write-gen bump; the re-entry guard fires in debug and is absent in release;
push/remove correctly triggers F5 list GC.

---

# Phase RD4 — Handler and widget integration *(no widget API changes)*

The measure of success for this phase is that **`lumen-widgets` does not change**.
If the derive requires touching the widget API, the front-end claim is false.

## Steps (each independently green)
- **RD4.1 — Handler adapter.** Handlers are `Rc<dyn Fn(&Runtime)>` retained on the
  node graph. Add an adapter so `|s: &mut Write<App>| …` is accepted: the closure
  captures nothing and reconstructs `Write<App>` from the `&Runtime` it is handed.
  The retained handler type is unchanged, so ADR-013, the F2 handler-currency check,
  and F4's `input.invokeAction` are all untouched.
- **RD4.2 — `stable_handler!` compatibility.** Because `Write<T>` is constructed
  from `&Runtime` and captures nothing, derive-style handlers remain `Copy`. Add
  passing and `compile_fail` doctests alongside the existing ones.
- **RD4.3 — Sugar.** `text!` and `bind!` accept field accessors, producing
  `Dynamic<T>` closures over `&Runtime` exactly as today.
- **RD4.4 — Agent verbs on derive state.** Confirm `ui.getDeps` reports
  `"todos[47].done"`, and extend `introspection_f4.rs` conformance to cover a
  derive-based app. Per-prop dep reporting through field paths is *more* legible
  than interned key strings — verify that in the conformance output, don't assume it.

*Acceptance:* an unmodified widget from `lumen-widgets` accepts a derive-style
handler; `input.invokeAction` drives it; F4 conformance passes on a derive app.

---

# Phase RD5 — Migration, docs, and the default question

## Steps (each independently green)
- **RD5.1 — Port two examples.** One simple, one list-heavy (to exercise RD2.3 and
  RD3.5). Leave the other 49 on `cx.signal`. Both styles green in the same
  workspace **is** the proof of the foundational invariant.
- **RD5.2 — Skills.** `building-apps` gains the derive path in its state rules.
  `writing-widgets` should need **no change** — if it does, RD4 failed.
- **RD5.3 — Doc currency (AGENT.md, binding).** Update `.ai_docs/02-spec-core.md`
  §4, the matching checkbox in `.ai_docs/06-task-graph.md`, and add an ADR-011 note
  that snapshot keys may now be field paths.
- **RD5.4 — Decide the default.** Whether `lumen new` scaffolds the derive style is
  a separate call, made *after* RD5.1 gives real ergonomics data. Do not
  pre-commit it here.

*Acceptance:* both authoring styles pass CI; docs and skills current per the
doc-currency rule.

---

## Sequencing

```
RD0 ── RD1 ── RD2 ──┬── RD4 ── RD5
                    └── RD3 ──┘
```

RD2 and RD3 can proceed in parallel once RD1's identity primitive lands. RD0 is
strictly first — it is cheap and it converts the rest into transcription.

| phase | size | risk |
|---|---|---|
| RD0 | S | none (spike) |
| RD1 | M | **const-hash equivalence** — highest, but bounded and testable |
| RD2 | M | low (uses existing read machinery) |
| RD3 | M | **re-entry via `DerefMut`** — new hazard, needs the debug guard |
| RD4 | S/M | low if the front-end claim holds; if not, stop and reassess |
| RD5 | S | none |

## ADR impact

| ADR | effect |
|---|---|
| ADR-003 (deps) | **None.** `syn`/`quote`/`proc-macro2` are already whitelisted and already `lumen-macros` dependencies. No escalation. |
| ADR-007 (signals) | **Unchanged.** Still fine-grained signals; this is a front end. |
| ADR-011 (snapshot) | **Note needed** — keys may be field paths. Round-tripping is an RD1.4 gate. |
| ADR-013 (no closures in state) | **Unchanged** — RD4.1 preserves the retained-handler type. |
| ADR-021 (hash identity) | **Extended, not revised** — field paths are another source of `IdHash`, folded the same way. |

## Risks

1. **Const-hash divergence (RD1.1).** Highest risk, fully bounded: the corpus test
   either passes or the `OnceCell` fallback ships. Either way the invariant holds.
2. **Re-entry through `DerefMut` (RD3.3).** The ergonomic win creates a new way to
   violate update purity. Debug-mode guard is mandatory, not optional.
3. **Two authoring styles in one codebase.** Real cost to docs and to agent-authored
   code, which now has two correct answers to "how do I hold state." RD5.4 exists to
   resolve this deliberately rather than by drift.
4. **rust-analyzer on derive-generated accessors.** Moderate — derives are far
   better supported than DSLs (this is a large part of why design A's `#[view]`
   body-parsing macro was rejected), but verify completion works on `s.count()`
   before RD5.1.

## Escalations

- If RD4 cannot be done without changing the widget API, the "pure front end"
  premise is broken. Stop and re-plan; do not proceed to RD5 with a forked widget
  surface.
- If RD0.3 finds the read-spelling overhead is not within noise of a direct
  `Signal::get`, reconsider — the ergonomic win does not justify a measurable
  read-path regression on a 42.9 ns idle frame.
