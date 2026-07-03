# Plan: fine-grained retained view (completing ADR-007) with observability as a first-class projection

*Design + build plan, 2026-07-02. Companion to `plan-rendering-performance.md`
(the paint/damage seam this sits above) and to the reactive store in
`lumen-core/src/state.rs`.*

> **Status (2026-07-03).**
> **F0 ✅ done** — `Headless::rebuild_fresh` (oracle) + `assert_view_coherent` +
> `Runtime::is_quiescent` and a `pump` fixpoint `debug_assert` (holds across the
> whole suite). **F1 ✅ done** — `BuildCx::scope` memoized subtrees: per-signal
> `Slot.version` + `Runtime::collect_reads`/`ReadSet`; a write re-runs only the
> scopes that read it (proven by run-count tests); scope-local signal
> namespacing; caches cleared on force/visual rebuilds. Perf unregressed (idle
> 20ns). **F2 🟡 partial** — nested-scope coherence + a 60-round randomized
> coherence fuzz (validates the all-collectors invalidation), **plus the
> handler-currency check (step 3)**. Two decisions resolved the earlier
> escalations (decision log 2026-07-03):
> - *Incremental layout (step 1) — SKIPPED.* One `TaffyTree` can't be partially
>   re-solved across disjoint subtrees (R4), so full-tree layout stays; the
>   O(changed) story is F1's build memoization + R2's damage paint. The
>   separate-`TaffyTree` split is an out-of-scope future task; F2's
>   retained-node-graph step is descoped accordingly.
> - *Handler-currency lint (step 3) — DONE* via a new `lumen-macros` proc-macro
>   (ADR-003 amendment: `syn`/`quote`/`proc-macro2`). `stable_handler!` asserts
>   the handler is `Copy` (may capture only stable Copy state, never an owned
>   snapshot); re-exported as `lumen_widgets::stable_handler`; passing +
>   `compile_fail` doctests. Catches owned-state captures, not Copy indices.
>
> *Semantics projection (step 2) — DONE* (adapted): with layout skipped,
> render/semantics already can't drift, so the projection delivered is the
> *reactive* structure — each `cx.scope` root carries its signal dependency
> keys into `SemanticsNode.deps` (and `ui.getLayout`), the foundation F4's
> `getDeps` reads. **F3 ✅ done (option B)** — `Dynamic<T>`/`Prop<T>` binding
> primitive; bindable `Element` text + background (`bind_text`/`bind_background`)
> evaluated during build with per-prop deps merged into `SemanticsNode.deps`;
> the `text!(cx, "…{sig}…")` sugar; and the **surgical retained patch**: a
> paint-only (background) binding change patches its node + repaints via R2
> damage with no rebuild/relayout/scope-re-run (isolated reads +
> `structural_reads` + `patch_bg_bindings`; `replay_reads` fixes the F1×F3.4
> skipped-scope interaction). Size-affecting (text) bindings rebuild (F1-memoized)
> — retained incremental layout stays out of scope (taffy skip). Guarded by the
> F0 oracle + an 80-round mixed fuzz. **F4 📋 planned** (not implemented) —
> additive agent verbs over the reified graph: `ui.getDeps` (per-prop),
> `ui.whatDependsOn` (predictive reverse index), `ui.lastChange` (change
> attribution), `input.invokeAction` (geometry-free actuation) + MCP entries;
> full sub-plan in the F4 phase below.

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
- **Authoring API** — F1–F2 are **transparent** (framework memoizes scopes;
  author code unchanged, `cx.scope` optional). F3 (per-property bindings) *does*
  change the authoring surface. **Resolved (2026-07-03, decision log): option B —
  author-expressed bindings, with `lumen-macros` sugar** (`text!`/`For`), chosen
  because the framework is pre-1.0 with no consumers and declared bindings beat
  inferred holes for observability. Un-bound `build -> Element` code still
  compiles (props default to `Static`); F3 details are in its phase below.

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

# Phase F3 — Per-property bindings (true model c)

**API decision (2026-07-03): option B — author-expressed bindings, with macro
sugar** (decision log). Rationale: the framework is pre-1.0 with no external
consumers, so the API is free to change; on the merits, *declared* bindings beat
*inferred* holes for an AI-first framework — the reactive graph the agent
inspects == the graph the author wrote == the graph that drives updates, with no
third inferred version to drift. Declared boundaries also make the once-vs-
reactive distinction syntactically honest (unlike A, where `build` looks
immediate-mode but runs once).

## The model

The view is built **once**; a dynamic prop is a **binding** — a small
`(deps, project)` derivation that re-runs *only that prop* when its deps change,
never the surrounding build. Two distinct kinds of "dynamic", designed apart on
purpose:

- **Binding (a *value* changes).** A `Dynamic<T>` = `Rc<dyn Fn(&ReadCx) -> T>`
  wrapping a reactive closure. An `Element` prop that can vary holds
  `Prop<T> = Static(T) | Dynamic(Binding<T>)`. `text!(cx, "Count: {count}")`
  expands to a `Dynamic<String>` capturing `count`.
- **Structure (the *tree shape* changes).** A list growing/shrinking, a
  conditional subtree — handled by a **keyed scope** (`For`/`cx.scope` with
  explicit identity, F1's primitive), which re-runs the scope body to add/remove
  nodes. Bindings never change structure; scopes never patch a leaf prop. The
  `text!`-vs-`For` split is the authoring rule, documented from day one.

## Authoring surface (`lumen-macros`)

The ergonomic tax of B (`text(Dynamic::new(move |c| format!("{}", count.get(c))))`)
dissolves with sugar, emitted by the proc-macro crate already in the workspace:

```rust
// value binding — reactive hole in a string:
text!(cx, "Count: {count}")            // → Dynamic<String> depending on `count`
// prop bindings:
node.class(class!(cx, if active { "on" } else { "off" }))
node.opacity(bind!(cx, fade.get(cx)))
// structure — keyed list (re-runs body per item, patches props within):
For::keyed(cx, items, |cx, item| row!( text!(cx, "{item.name}") ))
```

The macro records each binding's dep keys directly (it knows the captured
signals), so the `deps` projection (F2 step 2) becomes exact per-prop rather than
per-scope.

## Retained node graph (reopened here — *minus* incremental layout)

"Build once, patch props" requires **retaining the node graph** (F2 step 1,
skipped earlier for layout reasons). B reopens node retention but **not**
incremental layout — those are separable:

- **Paint-only prop change** (color, class, opacity, transform, fixed-size
  content): patch the retained node's field → mark one paint tile via R2 damage.
  *Fully surgical*, no layout, no rebuild.
- **Size-affecting prop change** (text content, show/hide): patch the field →
  full-tree layout (taffy skip stands) → R2 damage paint. Surgical build+paint,
  full layout. Accept this until/unless the separate-`TaffyTree` split is done.

So the retained tree persists across pumps; bindings patch fields in place; the
old `build_node` full rebuild remains the coherence oracle (`rebuild_fresh`).

## Coherence & observability extensions

- **Oracle (F0) extends per-prop.** `assert_view_coherent` already compares the
  whole display list + semantics vs `rebuild_fresh`; with bindings it gains
  teeth at prop granularity — a binding that patches the wrong field (or forgets
  the semantic projection) diverges from a fresh build and fails.
- **Anti-drift (F2 §2) stays structural.** A binding updates a node field; the
  semantic projection reads the *same* field, so render + a11y + agent can't
  desync (the binding fans out to both, by construction — not two code paths).
- **`getDeps` (F4) becomes exact.** Per-prop bindings carry their own
  subscriptions, so the agent can answer "what does *this prop* depend on", and
  change-attribution reports exactly which props re-ran on a write.

## Steps (each independently green, gated by F0)

1. `Prop<T>` + `Dynamic<T>`/`Binding` in `lumen-core`; `Element` props that can
   vary become `Prop<T>` (default `Static`, so un-bound authoring is unchanged).
2. Retain the node graph on `Headless`; a first build populates it; subsequent
   pumps apply bindings whose deps changed (via `ReadSet`), patching node fields
   + marking R2 damage; size-affecting fields flag a full relayout.
3. `text!` / `class!` / `bind!` / `For` macros in `lumen-macros` (emit bindings +
   exact dep keys; `stable_handler!`'s HRTB technique reused for closures).
4. Semantic projection reads the patched fields (F2 §2 fan-out); per-prop `deps`.
5. `ui.getDeps` + change-attribution (rolls into F4).

## Acceptance
Harness green (incremental == `rebuild_fresh`, now at prop granularity); "recolor
one node" updates exactly one field + one paint tile with **no** layout and **no**
scope re-run; "edit one text" patches the field + relayouts (full-tree) + one
tile; `getDeps` returns the exact per-prop subscription set; existing
`build -> Element` code (no bindings) still compiles and behaves as today.

# Phase F4 — Agent introspection of the reified graph *(additive protocol)*

**Goal.** Turn the reactive graph F0–F3 built into agent-answerable questions:
*what does this node depend on*, *what will change if I set signal X*, *what
just changed and why*, and *activate this control without pixels*. All additive
optional JSON-RPC methods + MCP entries — no change to existing verbs or the
`lumen-semantics/1` schema (§2-safe). The data already exists; F4 is mostly
exposure + one reverse index + recording what the pump did.

**Foundation already in place (F0–F3).** Per-node `SemanticsNode.deps`
(scope + binding dep keys) is in `ui.getLayout`; `structural_reads`, per-binding
`ReadSet`s (`bg_bindings`), and scope `ReadSet`s exist; `NodeMeta` holds the live
handlers; `pump` already routes a change to *patch* vs *rebuild*. So F4 reads
these out rather than computing anything new.

## F4.1 — `ui.getDeps { selector }` (per-prop dependency query)

The node's deps are already merged onto `SemanticsNode.deps`. F4.1 adds the
*breakdown*: retain the split in `build_node` (`scope`, `text`, `background`
dep-key lists) instead of merging into one, in a per-node `NodeDeps`.

- Verb → `{ node, deps: [union], byProp: { scope: [...], text: [...], background: [...] } }`.
- **Change:** replace `merge_deps` with a retained `NodeDeps` on the node (union
  still projected to `SemanticsNode.deps` for back-compat); `ui.getDeps` reads it.
- **Verify:** a text+bg node reports `byProp.text` and `byProp.background`
  distinctly; union equals today's `deps`.

## F4.2 — Reverse index + `ui.whatDependsOn { signal }` (predictive)

Invert the per-node deps into `signal-key → [nodes]`, built once per rebuild.
Answers "if I write X, what updates" **without** writing — and classifies each
dependent by how it would update (from F3.4's split):

- Verb → `{ signal, dependents: [{ node, via: "scope"|"text"|"background", update: "rebuild"|"patch" }] }`
  (`background` ⇒ `patch`; `scope`/`text` ⇒ `rebuild`).
- **Change:** `Headless` builds a `HashMap<String, Vec<Dependent>>` at the end of
  rebuild from the retained `NodeDeps` + `bg_bindings`.
- **Verify:** `whatDependsOn(sig)` lists exactly the nodes whose `deps` contain
  `sig`, with the right `update` class; empty for an unread signal.

## F4.3 — Change attribution (`ui.lastChange`)

Record what the last `pump` actually did, so the agent can confirm cause→effect
after a write. `pump` already branches (rebuild / `patch_bg_bindings` / idle);
have each branch record a `ChangeReport`.

- `patch_bg_bindings` records the patched node ids (`update: "patch"`).
- a structural rebuild records the scopes that re-ran (the F1 cache already knows
  skipped-vs-run) and whether it was a full rebuild (`update: "rebuild"`).
- Verb `ui.lastChange` → `{ kind: "idle"|"patch"|"rebuild", nodes: [...], scopes_reran: n }`;
  also fold a compact form into `FrameStats`.
- **Verify:** set a bg signal → `lastChange.kind == "patch"`, `nodes == [that node]`;
  set a scope signal → `kind == "rebuild"`, that scope in `scopes_reran`; and the
  reported node set matches a display-list diff vs the pre-write frame.

## F4.4 — `input.invokeAction { selector, action }` (geometry-free actuation)

Activate a control by running its retained handler directly, instead of
synthesizing a pointer at `center(bounds)` and re-hit-testing (fragile under
overlap/transforms). The handler lives in `NodeMeta` (`on_click`, …).

- New `Headless::invoke_action(selector, Action) -> Result<..>`: resolve selector
  → `NodeIndex` → call `meta[node].on_click(&rt)` for `Click` (focus/dismiss for
  those actions), then `pump()`.
- Verb `input.invokeAction { selector, action }` → `{ ok, node }`. Sibling to
  `input.click`; more robust (no pixel/hit-test).
- **Verify:** `invokeAction(#inc, "click")` drives the counter identically to
  `input.click`, and still works when the target is occluded (where a pixel click
  would miss).

## F4.5 — MCP manifest + conformance

- Add `tool()` entries: `ui_getDeps`, `ui_whatDependsOn`, `ui_lastChange`,
  `input_invokeAction`.
- Conformance (over an example): `whatDependsOn(X)` predicts a node set; write
  `X` + pump; `lastChange` reports the same set; a display-list diff confirms it;
  `invokeAction` drives a handler. Asserts the predictive, actual, and coherence
  views all agree.

## Acceptance
Four additive verbs + MCP entries; existing verbs/schema unchanged. `getDeps`
gives per-prop subscriptions; `whatDependsOn` predicts and `lastChange` confirms
the *same* node set (cross-checked against a frame diff); `invokeAction` actuates
geometry-free. No new deps; the reactive graph the agent queries is the one the
author declared (F3) and the runtime drives.

## Not in F4 (follow-ons)
`class!`/`bind!`/`For` authoring sugar (F3 tail); the **ADR-013 wording
amendment** now due (handlers "re-registered each build()" → "re-created when the
owning scope re-runs"); and the separate-`TaffyTree` split for incremental layout
(if a real workload ever makes full-tree layout the bottleneck).

---

# Sequencing

```
F0  Coherence oracle + harness (rebuild_fresh, pump fixpoint)  ── gates everything
      │
F1  cx.scope memoized subtrees (transparent, opt-in)  ── ships value alone
      │
F2  Retained node graph + semantics projection + handler lint  ── the pivot
      │
F3  Per-property bindings (option B) ── API decided; reopens node retention
      │
F4  getDeps / change-attribution / invokeAction  ── additive verbs
```

F0 first, always. **F1 is shippable on its own** and delivers most of the
large-app perf with the least risk (it never abandons full rebuild). F2 is the
retained-view pivot (incremental layout skipped; observability projection +
handler lint done). F3 changes the authoring API (option B — author-expressed
bindings + macro sugar, resolved in the decision log) and reopens node retention
(build once, patch props) *minus* incremental layout. F4 rides on F3's per-prop
reified graph.

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
- **Authoring-API change at F3.** → Resolved (2026-07-03): option B (author-
  expressed bindings + `lumen-macros` sugar); acceptable because the framework is
  pre-1.0 with no consumers. Un-bound `build -> Element` still compiles (props
  default `Static`).

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
