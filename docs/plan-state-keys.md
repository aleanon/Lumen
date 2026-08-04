# Plan: state-key safety (K-series)

*Design + build plan, 2026-08-04. Replaces the retired
`docs/plan-reactive-derive.md`. Companion to `plan-hash-identity.md` (ADR-021,
whose `impl Hash + Debug` identity this builds on).*

> **Origin.** `plan-reactive-derive.md` proposed a Xilem-shaped
> `#[derive(Reactive)]` front end, justified as relief from string-key
> discipline. Three reviews retired it (see the box at the top of that file): its
> identity is root-only, it needs its own ADR per the ADR-021 decision entry, and
> — decisively — it does **not** deliver key-discipline relief, because
> widget-owned state is string-keyed public API. This plan attacks the stated
> problem directly instead, at a fraction of the cost, with one authoring style
> and no ADR.

---

## What is actually broken (measured, 2026-08-04)

A probe against the real `Runtime` establishes the failure modes precisely.
Both matter, and they are **not** the same problem:

**1. Same key, different type — silent corruption, then a useless panic.**

```
let a = rt.signal("x", || 0i32);
let b = rt.signal("x", || String::new());   // no complaint
b.set(&rt, "hello".into());                  // succeeds; slot now holds a String
a.get(&rt);                                  // panic: "signal type mismatch"
```

`signal_at` returns a typed handle for an existing slot without checking the
type, and `set_value` **replaces the boxed value wholesale**. The corruption is
silent until some later read downcasts and fails — at which point the panic
(`state.rs`, `read_with`) says only `"signal type mismatch"`: no key name, no
expected type, no found type, no diagnostic code, and no indication of which of
the two call sites was wrong. This is the genuinely undetected failure, and it is
unambiguously a bug in every case.

**2. Same key, same type — deliberate, and must stay silent.**

```
let a = rt.signal("cart.open", || false);
let b = rt.signal("cart.open", || false);    // shares the slot, on purpose
```

This is the widget contract: `Sheet`, `Drawer`, `Popover`, `Combobox`,
`PickList`, `ColorPicker`, `Pagination`, `RangeSlider`, `FilePicker` all expose
state as `{name}.open` / `{name}.selected` / `{name}.page` so app code can drive
them (`sheet.rs:30`, and documented in `building-apps`). **A blanket
duplicate-key warning would fire on correct code constantly** and must not be
built.

> **Correction to the retirement reviews.** Two of them recommended "revive the
> dead W0001 duplicate-key diagnostic." W0001 is **not dead** — `audit.rs:121`
> emits it and `lint.rs:57` / `two_instances.rs:41` test it — and it is about
> duplicate **`StableId`** (`.id("save")` selector names), not signal keys. The
> `building-apps:159` line calling it dead, and the
> `review-docs-vs-code-2026-07.md:78` line calling it never-emitted, are both
> **stale**; fixing them is K3. W0001 is a different diagnostic for a different
> problem and needs no work beyond K3's doc correction.

---

## Thesis

Key *invention* is a smaller problem than the retired plan claimed, because
ADR-021 already accepts `impl Hash + Debug` and `building-apps` already
recommends `enum Field { Row(u32) }`. Rust also already rejects duplicate enum
variants at compile time, so the "duplicate check" a derive would add is free.

So this plan does **not** try to replace how keys are spelled. It does two
things:

1. **Make the one real failure loud and legible** (K1) — the highest
   value-per-line change available, and it helps all ~391 existing `.signal(`
   call sites immediately.
2. **Make a key space enumerable** (K2) — the modest, honest scope for a derive:
   not "compiler-checked names" (an enum already gives that), but a
   `const ALL: &[Self]` that enables a startup collision audit and lets the agent
   report an app's full key vocabulary, including keys never yet read.

## Foundational invariant (do not violate)

> **No new diagnostic may fire on the widget re-addressing contract.** Any check
> added here must be silent for `cx.signal(k, …)` called twice with the same key
> **and** the same type. That pattern is load-bearing public API across at least
> 13 widget files.

`crates/lumen-widgets/tests/two_instances.rs` and the `widget_gallery` example
are the standing proof; a false positive there fails the phase.

---

# Phase K1 — Typed key-collision diagnostic *(S — do this first, it stands alone)*

## Steps (each independently green)
- **K1.1 — Record the type in the slot.** Add `type_name: &'static str` (from
  `std::any::type_name::<T>()`, zero runtime cost, `&'static`) to `Slot` at
  creation in `signal_at`.
- **K1.2 — Check on re-address.** When `signal_at` finds an existing slot whose
  recorded type differs from `T`, emit a new diagnostic — **`W0003`, allocated in
  `lumen-core/diagnostics.md` and `codes`** — naming the readable key, the
  existing type, and the requested type. `id_to_key` already holds the readable
  name, so the message costs nothing on the hot path (the check is a pointer
  compare on `&'static str`).
- **K1.3 — Decide warn vs panic, and say why.** Recommendation: **warn, don't
  panic.** A collision is always a bug, but panicking at *address* time would
  turn a currently-survivable mistake into a hard failure at a call site that may
  be in a widget the author doesn't own. The later downcast still panics; K1.4
  makes that panic useful.
- **K1.4 — Make the downcast panic legible.** Replace `read_with`'s bare
  `.expect("signal type mismatch")` with the key name, expected type, found
  type, and a pointer to `W0003`. Same for the write path.
- **K1.5 — Tests.** A collision emits exactly one `W0003` naming both types; the
  same-key/same-type widget pattern emits **none** (the foundational invariant);
  the downcast panic message contains the key name.

*Acceptance:* the probe at the top of this document produces a named `W0003`
instead of silence, and its eventual panic names the key; `two_instances.rs` and
the widget suite stay diagnostic-free.

---

# Phase K2 — `#[derive(StateKey)]` *(S/M — only worth it for the enumerable key space)*

## Be honest about what this adds

`enum Field { Row(u32) }` already works today with zero new machinery, derives
`Hash + Debug`, and is already the recommended pattern. A derive adds exactly
three things, and the plan should not pretend otherwise:

| claimed | real? |
|---|---|
| compiler-checked names | already true — enum variants are checked |
| duplicate-variant rejection | already true — rustc rejects duplicate variants |
| **enumerable key space (`const ALL`)** | **new** |
| **startup collision audit over the declared space** | **new, enabled by the above** |
| **total dep vocabulary for the agent** | **new, enabled by the above** |

If K2's steps below don't justify themselves on those last three, **don't build
it** — K1 is the load-bearing phase and ships alone.

## Steps (each independently green)
- **K2.1 — The derive.** `#[derive(StateKey)]` on a fieldless enum emits
  `const ALL: &[Self]` plus a `key_name()` matching `identity::key_name`'s
  output, so declared names and interned names agree. Fieldless only in K2.1;
  variants with payloads (`Row(u32)`) can't be enumerated and stay supported as
  plain `Hash + Debug` keys, unchanged.
- **K2.2 — Startup collision audit.** A debug-build check that folds every
  `ALL` entry under a given scope and asserts no two produce the same `IdHash`.
  At 128 bits this will never fire on real input — which is the point: it is a
  cheap standing proof that the identity scheme holds, in the same spirit as
  ADR-021's decision to drop the collision probe.
- **K2.3 — Agent vocabulary.** Expose the declared key space through the
  existing dep-reporting surface so `ui.whatDependsOn` can answer for keys that
  have never been read. This is the one genuine benefit the retired plan's
  "static dep graph" argument identified, obtainable here for a fraction of the
  cost.
- **K2.4 — Skill update.** `building-apps` gains the derive as an *option* under
  the existing typed-key guidance — **not** as a second authoring style. One
  authoring style is the whole point of preferring this plan.

*Acceptance:* an app using `#[derive(StateKey)]` produces byte-identical store
state to the same app using a bare enum key (the equivalence bar the retired
plan set, here trivially met because nothing about addressing changes);
`ui.whatDependsOn` answers for an unread declared key.

---

# Phase K3 — Fix the stale W0001 documentation *(XS)*

Three documents currently misdescribe a diagnostic that works. This is exactly
the drift AGENT.md's doc-currency rule exists to prevent, and it directly caused
two of the retired plan's reviews to recommend reviving something that already
ships.

- **K3.1** `.claude/skills/building-apps/SKILL.md:159` — "Duplicate-id detection
  is not enforced yet (W0001 dead — plan W.4)" is wrong. W0001 is emitted by
  `audit::lint` (`audit.rs:121`). Correct it, and state the real limitation:
  the audit runs from the lint path (`ui.lint` / `audit_actions`), **not** during
  a normal `pump`.
- **K3.2** `docs/review-docs-vs-code-2026-07.md:78` — lists W0001 under "defined
  but never emitted". Correct it.
- **K3.3** Decide and record whether the duplicate-`StableId` audit *should* run
  in the dev-loop pump rather than only on demand. Cheap either way; the point is
  to have an answer written down instead of three contradictory ones.

*Acceptance:* no document claims W0001 is dead; `skills-smoke` still green.

---

## Sequencing

```
K1 (standalone, ship first) ── K2 (only if the ALL-based benefits justify it)
K3 (XS, anytime — do it with K1)
```

K1 is the whole value proposition and does not depend on K2. K2 is genuinely
optional and should be killed if K2.2/K2.3 don't earn it.

| phase | size | value |
|---|---|---|
| K1 | S | high — turns silent corruption into a named diagnostic, helps all ~391 existing sites |
| K2 | S/M | modest — enumerable key space only; kill if unearned |
| K3 | XS | removes the drift that misled two reviews |

## ADR impact

**None.** ADR-021 already accepts `impl Hash + Debug`; ADR-007 is untouched;
no reactivity-model change, so the ADR the decision log reserved for a
`&mut AppState` design is not needed here. `W0003` is a new diagnostic code,
which `ADR-019`'s registry process already covers (allocate in
`lumen-core/diagnostics.md`, never invent inline).

## ADR-003 / determinism

No new dependencies. K2 uses `syn`/`quote`/`proc-macro2`, already whitelisted and
already `lumen-macros` dependencies. K1's `type_name` is `core`.

## Explicit non-goals

- **A second authoring style.** The retired plan's central mistake.
- **A blanket duplicate-key warning.** Forbidden by the foundational invariant —
  same-key/same-type re-addressing is the widget contract.
- **The Xilem `&mut AppState` model.** Still possible, but it needs its own ADR
  and an honest justification as an authoring-model change. It is not
  key-discipline relief; see `plan-reactive-derive.md`'s retirement box.
