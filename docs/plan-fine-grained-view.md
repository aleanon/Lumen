# Plan: fine-grained retained view (completing ADR-007) with observability as a first-class projection

*Design + build plan, 2026-07-02. Companion to `plan-rendering-performance.md`
(the paint/damage seam this sits above) and to the reactive store in
`lumen-core/src/state.rs`.*

> **Why this exists.** ADR-007 already commits the framework to *"fine-grained
> signals (Solid-style), no VDOM/diffing … O(changed) updates."* The headless
> runtime does **not** yet honor that: the T0.9 amendment (2026-06-15) records
> that it *"does a full rebuild every `pump` (M0 simplicity; fine-grained
> signal-driven rebuild is a perf refinement for later)."* So the view layer runs
> `build(cx) -> Element` top-to-bottom on every change (`app.rs:1092`), reading
> signals **untracked** (`Runtime::tracks() == false`) and gating rebuilds on one
> global `write_gen` counter (`app.rs:408`). This plan finishes ADR-007: it makes
> the view a *retained* reactive graph updated in O(changed), **without** losing —
> in fact while strengthening — the agent's ability to observe and drive the app.

---

## The thesis (read this first)

Today the agent's view of the UI — `ui.getTree`/`getLayout`/`getStyles`/`lint` —
is a **byproduct of rebuilding**: the semantics doc is a fresh snapshot of the
transient tree `build` reconstructs each frame. Fine-grained reactivity's whole
purpose is to *stop* rebuilding, which looks like it removes the tree the agent
reads. It does the opposite:

- A fine-grained view **retains** its node graph (persistent, stably addressable)
  instead of throwing it away 60×/second. A retained tree is *easier* to observe.
- The dependency graph is **reified**: every dynamic hole is an explicit
  subscription (signal → node prop). Questions that are unanswerable today —
  *"what depends on signal X?"*, *"why did this node change?"*, *"what will change
  if I set X?"* — become first-class. For an AI-first framework this is the single
  best reason to do the pivot.

The price: observability stops being a free byproduct of rebuild and becomes a
**first-class projection** that the same bindings which update the render must
also update. Get that structural, and render + a11y + agent tree cannot desync
(honoring ADR-009: one tree, four consumers).

## Foundational invariant (do not violate)

> **The retained view is a pure function of the durable store.** For any sequence
> of writes, the incrementally-updated tree MUST equal the tree produced by a
> fresh rebuild from the final state:
>
> ```
> incremental(state₀, [w₁..wₙ])  ==  rebuild_fresh(stateₙ)
> ```

This one invariant underwrites *five* things at once — they are all the same
operation, "reconstruct the view from the store":

| Consumer | Uses the invariant as |
|---|---|
| Snapshot / restore (ADR-011) | restore = load state → `rebuild_fresh` |
| CPU golden (ADR-002) | golden is rendered from `rebuild_fresh` |
| Replay determinism (agent drive loop) | replay events → same tree |
| Hot reload (ADR-012) | code reload = `rebuild_fresh` on file-watch |
| **Fine-grained coherence (this plan)** | incremental must equal `rebuild_fresh` |

Build the coherence harness **once** (Phase F0) and it pays for all five. Every
later phase is gated by it.

---

## The model

Three concepts, layered on the reactive runtime that already exists (the
subscription graph is proven: `write_one_of_many_reruns_exactly_one_scope`).

1. **Reactive scope (`cx.scope(id, |cx| …)`).** A region of the view that (a)
   subscribes to the signals it reads, (b) owns an identity path (so its
   signals are namespaced — this *also* fixes the flat-key-namespace /
   component-local-state gap), and (c) retains the `Element` subtree it produced.
   A write re-runs only the scopes that read the written signal.

2. **Retained node graph.** The `Element` tree persists across frames. A scope
   re-run patches *its* subtree in place; untouched subtrees are reused by
   reference. Node identity is the scope path + key (stable across updates — an
   upgrade over today's per-frame re-derivation).

3. **Per-property binding.** A dynamic prop (a label's string, a `.class`, a
   style value, computed bounds) is a `(deps, |v| -> prop)` derivation. When its
   deps change it updates *that one prop* on *that one node* — no scope body
   re-run, no subtree rebuild. This is true model (c).

**Lifetime separation (already true, and load-bearing).** The store is durable
and serializable; the reactive graph (subscriptions, effect/binding closures) is
"runtime-only and rebuilt each frame" (state.rs module docs). That seam is
exactly what makes both fine-grained update *and* hot reload safe: tear the graph
down, rebuild from the store, the store carries the data across.

---

## Observability as a first-class projection

Each retained node carries render props **and** semantic props. A binding on a
render-affecting prop updates the semantic field *through the same node*, so they
cannot drift:

- `ui.getTree` / `getLayout` walk the retained graph (always current).
- `getStyles` reads the retained node's computed style.
- lint / ink / overflow run geometry over the retained tree.
- the state snapshot is **unchanged** (the store is orthogonal to view
  granularity) — the agent's deepest observability survives untouched.

New, additive agent verbs the reified graph makes cheap (schema-additive per the
escalation rules — new optional methods, no breaking change to docs 03):

- `ui.getDeps { selector }` → the signals a node's props subscribe to.
- change attribution: after a write, the set of nodes/props that updated
  ("why did this change") — exact, not a whole-tree diff.
- `input.invokeAction { selector, action }` → run the node's handler directly
  (geometry-free actuation, below).

## Agent interaction under a retained view

The drive loop is **act → settle → observe**, and it survives the pivot with
*less* work than observation:

- **act** — hit-testing needs a tree with bounds + handlers; the retained graph
  has one, with *stable* targets that don't invalidate each frame. Plus a new
  geometry-free path: invoke a node's semantic action directly (run its handler)
  instead of synthesizing a pixel and re-hit-testing — more reliable AI actuation.
- **settle** — the quiescence point "rebuild finished" becomes "reactive graph
  reached a fixpoint," which the runtime already implements (`Runtime::flush`
  drains dirty scopes to empty). `pump` = drain input → dispatch → flush to
  fixpoint → reconcile render+semantics → return. Same synchronous barrier.
- **observe** — read the projected semantic tree (above).

**The one real hazard: handler currency.** Today handlers are regenerated every
rebuild (ADR-013: *"handlers re-registered each build()"*), which silently masks
handlers that capture **transient build-time values** — the `todos`
`move |rt| v.remove(i)` index-capture. In a retained model a handler attached
once must not close over a stale `i`. The rule the pivot forces: **handlers
capture reactive identity (signal handles, which are `Copy` and always read
current state; and stable keys), never positional snapshots.** Handlers that
genuinely depend on build-time values live in a scope that re-creates them when
those deps change. This is a *net positive*: it kills a latent correctness bug
that rebuild currently hides, and it is the same "capture identity, not position"
fix that fine-grained lists and component-local state both want. Enforced by a
lint (below).

---

## Reconciliation with existing ADRs (and what must escalate)

- **ADR-007** (fine-grained, no VDOM) — this plan *implements* it. No conflict.
- **ADR-009** (one tree = a11y = locator = agent) — the projection *strengthens*
  it (structural fan-out makes drift impossible). No conflict.
- **ADR-011 / ADR-012** (snapshot / hot reload) — unchanged; both become
  consumers of the F0 invariant. Hot reload of *code* = `rebuild_fresh`; of
  *data/style* = incremental (a finer path, optional). No conflict.
- **ADR-013** (handlers re-registered each `build()`; no closures in stored
  state) — **needs a wording amendment**: "re-registered each build" becomes
  "re-created when the owning scope re-runs." Handlers remain on the *ephemeral*
  node graph, never in stored state, so the ADR's hard precondition holds. ⚠️
  *This is an escalation (§2 item 2/3 territory): record the amendment, don't
  silently redefine an ADR.*
- **Authoring API** — the goal is to keep `build(cx) -> Element` as the authoring
  surface (model (a)'s ergonomics) with the runtime doing fine-grained work
  underneath. Phases F1–F2 are achievable **transparently** (framework memoizes
  scopes; author code unchanged, `cx.scope` optional). Phase F3 (per-property
  bindings) is where an authoring-API question opens: either the framework
  *infers* holes by recording which signals each `Element` prop read during
  build, or authors express bindings explicitly. ⚠️ *The F3 API surface is an
  open escalation (post-1.0 API per §2 item 3) — F3 does not start until it is
  resolved in the decision log.*

---

# Phase F0 — Coherence oracle & harness *(do first; gates F1–F4)*

## Current state
`rebuild_inner` already is `rebuild_fresh` (a pure rebuild from the store). There
is no incremental path yet, so the invariant is trivially true — which is exactly
why F0 is cheap to stand up now and expensive to retrofit later.

## Steps (each independently green)
1. Name the oracle: expose `Headless::rebuild_fresh()` (rename/alias of today's
   full rebuild) as the canonical "tree from state."
2. Formalize the `pump` fixpoint contract: `pump` drains input, dispatches, then
   `flush`es the reactive graph to quiescence before returning; assert no dirty
   scopes remain (a debug invariant).
3. Coherence harness: `assert_view_coherent(app, [writes])` — apply writes via
   the (currently trivial) incremental path, then `rebuild_fresh`, assert the
   semantic trees + computed styles + display lists are byte-identical.
4. Wire the harness into CI over the gallery + a few examples (they must stay
   coherent as later phases introduce a real incremental path).

## Acceptance
Harness is green (trivially, pre-F1), runs in CI, and is the gate every later
phase's tests call.

# Phase F1 — Reactive scopes (`cx.scope`) — memoized subtrees *(low risk, most of the perf)*

## Target
"Only the changed region recomputes," on top of the existing full-rebuild, with
**no change to the authoring API** (scopes are opt-in; un-scoped code behaves as
today).

## Steps (each independently green)
1. `BuildCx::scope(id, |cx| -> Element)`: runs the closure inside a **tracking**
   read scope, caches the returned subtree keyed by the scope's signal deps.
2. On rebuild, a scope whose deps are unchanged returns its cached subtree by
   reference; changed scopes re-run. (Uses the runtime's existing exact-scope
   re-run.)
3. Scope-local state: `cx.scope` prefixes signal keys with its identity path, so
   a reused component gets its own state (fixes the flat-namespace gap) — and it
   is still a store signal, so it snapshots/restores normally.
4. Bench: a large view with one changed field re-runs only that scope (assert
   scope-run count == 1), vs. today's whole-tree rebuild.

## Acceptance
F0 harness green; bench shows O(changed-scopes) rebuild; gallery unchanged
visually (goldens stable); `cx.scope` documented with the "capture identity"
handler rule.

# Phase F2 — Retained node graph *(the structural pivot)*

## Target
The `Element`/node tree persists across frames; a scope patches its subtree in
place; semantics are **projected** from retained nodes (structural fan-out).

## Steps (each independently green)
1. Retain the built node tree on `Headless`; scope re-runs splice their new
   subtree into the retained tree at the scope's node.
2. Move semantic fields onto (or project them from) the retained node, updated by
   the same code path that updates render props — the anti-drift guarantee.
3. Handler-currency lint (**W-code, new**): flag a handler closure capturing a
   non-`Copy`, non-signal local (the `i`-index pattern) → fails CI instead of at
   runtime. Fix the gallery/examples to capture identity.
4. Extend the F0 harness: retained-incremental vs `rebuild_fresh` must match after
   arbitrary write sequences (the real coherence test now has teeth).

## Acceptance
Harness green with a *real* incremental path; lint clean across examples; agent
`getTree`/`getLayout` identical between incremental and `rebuild_fresh`.

# Phase F3 — Per-property bindings (true model c) *(gated on API escalation)*

## Target
Dynamic props update single node props without re-running the scope body;
incremental render + incremental semantics.

## Steps (sketch — details pending the API decision)
1. Represent a dynamic prop as `(deps, project)`; on dep change, update the one
   prop + its semantic projection + mark that node's paint dirty (feeds the R2
   damage system).
2. Incremental semantics: only changed nodes' semantic fields update.
3. `getDeps` + change-attribution verbs read the binding subscriptions.

## Acceptance
Harness green; "edit one field" updates exactly one node's prop + one paint tile;
`getDeps` returns the correct subscription set.

# Phase F4 — Agent introspection of the reified graph *(additive protocol)*

## Steps
1. `ui.getDeps`, change-attribution, `input.invokeAction` (all additive optional
   methods; schema stays back-compatible per §2).
2. MCP tool manifest entries.
3. Conformance: drive an example, assert change-attribution matches the nodes the
   harness says changed.

---

# Sequencing

```
F0  Coherence oracle + harness (rebuild_fresh, pump fixpoint)  ── gates everything
      │
F1  cx.scope memoized subtrees (transparent, opt-in)  ── ships value alone
      │
F2  Retained node graph + semantics projection + handler lint  ── the pivot
      │
F3  Per-property bindings  ── BLOCKED on API escalation (decision log)
      │
F4  getDeps / change-attribution / invokeAction  ── additive verbs
```

F0 first, always. **F1 is shippable on its own** and delivers most of the
large-app perf with the least risk (it never abandons full rebuild). F2 is the
real retained-view pivot. F3 is the only phase that may touch the authoring API
and must not start before the escalation resolves. F4 rides on F3's reified graph.

# Risks & mitigations

- **Incremental divergence (glitches, stale bindings).** → The F0 invariant +
  harness is the whole safety story; every phase is gated by it. Full rebuild is
  always available as the oracle.
- **Silent observability drift** (render updates, semantics doesn't). → Structural
  fan-out in F2 (semantics projected from the same node prop), not a separately
  maintained structure.
- **Stale handler captures.** → W-code lint in F2; the "capture identity" rule
  documented from F1.
- **ADR-013 wording drift.** → Escalate the amendment before F2 lands.
- **Post-1.0 authoring-API break at F3.** → F1/F2 are transparent; F3 gated on an
  explicit decision-log entry (prefer hole-inference to keep `build -> Element`).

# Acceptance (whole plan)

- ADR-007 honored: writes update the view in O(changed), proven by scope-run /
  node-update counts, not wall-clock alone.
- The F0 invariant holds across the gallery + examples in CI (incremental ==
  `rebuild_fresh`) — which simultaneously guards snapshot/restore, golden, replay,
  and hot reload.
- Agent parity: `getTree`/`getLayout`/`getStyles` identical between incremental
  and oracle; new `getDeps`/attribution/`invokeAction` verified.
- Hot reload unchanged for code reload (fires the oracle); optional finer path for
  data/style.
- Handler-currency lint clean; no closure captures of transient build-time values.
