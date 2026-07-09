# Plan: the retained pipeline (remediation Phase A)

*Sub-plan for `docs/plan-remediation-2026-07.md` Phase A — the engine
keystone. Design + build plan, 2026-07-09. Companion to
`plan-fine-grained-view.md` (F-series; this revives its descoped F2) and
`plan-rendering-performance.md` (the damage/paint seam below).*

Goal: changed-frame cost scales with **what changed**, not tree size — the
ADR-007 "O(changed)" commitment — and `.lss` layout properties become real.
Every step is gated by the F0 coherence oracle (`assert_view_coherent`:
incremental ≡ rebuild-fresh) and the R0 golden corpus (byte-identical
frames, or intentional re-approval).

## Verified baseline (2026-07-09 investigation)

- `pump` (app.rs:470-556): rebuild when `force_rebuild || visual_changed ||
  time_driven || (write_changed && !structural_current)`; on
  `force_rebuild || visual_changed` it calls `clear_view_caches()` first.
- **View closures cannot observe hover/focus/pressed.** `BuildCx`
  (element.rs) exposes no accessor; `Headless.hovered_id/focused_id/
  pressed` are applied *after* the closures run — node flags in
  `build_node` (app.rs:1726-1729), `.lss` state parts in `compute_styles`,
  focus ring/caret in paint. So the hover-path cache wipe protects nothing.
- `rebuild_inner` (app.rs:1337-1406) reconstructs per rebuild: fresh
  `Tree`, fresh taffy `LayoutTree` + full `compute`, `meta` map, then
  `compute_styles` (post-layout), semantics tree, dep-index.
- `relayout_subtree` exists (lumen-layout/src/tree.rs:80) with exactly one
  caller: a lumen-layout test.
- Memoized `cx.scope` hits deep-clone their cached `Element` (element.rs
  ~556); scope/signal keys are `format!`ed `String`s.

## A.1 ✅ done (2026-07-09) — Hover/focus/pressed stop wiping the scope caches (S)

**Change:** in `pump`, clear caches only for `force_rebuild`
(resize/scale/stylesheet/theme — inputs a closure *can* observe via
`cx`, conservatively kept); `visual_changed` still triggers a rebuild but
**reuses memoized scopes** (they cannot be stale — see baseline).

**Accept:** new run-count test — a `cx.scope`d subtree does not re-run
when pointer motion flips `hovered_id`, while the hovered node's
flag/semantics state does update; whole suite's `assert_view_coherent`
stays green; `scope_memo_one_of_many` unregressed.

**Non-goal:** skipping the rebuild itself (styling state parts + flags
still refresh through the F1-memoized rebuild). Skipping it entirely is
A.5's restyle-only path.

## A.2 ✅ core slice done (2026-07-09) — Styles before layout; `.lss` layout properties become real (L)

*Landed: resolution moved inline into `build_node` (pre-layout, per-node —
no ancestry needed for compound selectors; dynamic classes already merged),
with `display`/`flex-direction`/`width`/`height`/`gap`/`padding`/`margin`
merged into `LayoutStyle` (element < .lss). The old post-layout
`compute_styles` pass is deleted; `emit_pass`/`get_styles` consume the same
maps unchanged. Guarded by tests/lss_layout.rs (incl. the text-height rule
and the `:hovered` layout-rule relayout); goldens byte-identical (no in-repo
sheet used layout props). Remaining property coverage (per-side, flex-*,
justify/align, min/max, grid tracks, position/inset, overflow) folds into
Phase B.3/B.4 as planned.*

**Change:** split `compute_styles` into (a) *pre-layout* resolution of
each node's rule set from (role, id, classes, states, sheet) — merging
`.lss` layout declarations into the node's `LayoutStyle` **before**
`layout.compute` — and (b) the existing paint-time application. Parse
grid track lists + per-side box properties (the parser half of drift
D#11). Rule origin: element `LayoutStyle` field < `.lss` (documented;
matches cascade origin order once B.6 lands).

**Accept:** `.lss` `width/height/padding/gap/flex-*` visibly affect
layout (new layout-from-lss fixture suite); 04 §10 rows flip
parse-only→rendered; R0 corpus diff reviewed (layout changes only where
stylesheets used layout props — none in-repo do yet, so expect
byte-identical); styling-lss skill + 04 banner updated (doc-currency).

**Risk:** state-part-driven layout (`:hovered { width: … }`) makes hover
relayout-triggering — acceptable (it re-enters the normal rebuild path);
document as a perf note.

## A.3 — F2 revived: retain Tree/meta/semantics/dep-index across pumps (XL)

The heart. Retain last build's `Tree` + `meta` + semantics + dep-index;
on rebuild, re-run only dirty scopes (F1 already knows them) and **splice**
their subtrees, leaving untouched siblings' nodes, style resolutions,
semantics, and dep entries intact.

Steps (each oracle-gated, landable separately):
1. **A.3.1** Scope boundaries become tree anchors: record node-range per
   scope during `build_node` (scope id → contiguous node span + parent).
2. **A.3.2** Memo hits return `Rc<Element>`/`Cow` — stop the deep clone;
   `build_node` learns to *skip* re-lowering an unchanged scope subtree
   (reuse its retained nodes) instead of re-walking the cloned Element.
3. **A.3.3** Splice: a re-run scope lowers into fresh nodes that replace
   its span (generational NodeIndex reuse); parent links/z/document order
   fixed up; `meta`/semantics/dep-index updated for the span only.
4. **A.3.4** Intern scope/signal keys (id table / `SmolStr`) — kills the
   per-build `format!` churn (cheap once spans exist).
5. **A.3.5** Delete the "rebuild = fresh Tree" assumption behind a
   feature-flagged escape hatch for one release (`LUMEN_FULL_REBUILD=1`)
   to bisect coherence regressions; the oracle compares against
   rebuild-fresh every test anyway.

**Accept:** one-of-N-rows signal write re-lowers O(row) nodes (counted:
new `FrameStats.nodes_rebuilt`); `build_node`+semantics+dep-index time on
the gallery drops ~an order of magnitude on small changes; whole suite +
80-round F3 fuzz + coherence oracle green; goldens byte-identical.

## A.4 — Incremental layout (L, needs A.3)

With the tree retained, mark spliced spans' taffy nodes dirty and call
`relayout_subtree` (finally wiring its only-test-caller into the pump);
full `compute` remains for root-constraint changes (resize/scale).
Taffy's single-tree parent↔child sizing stays respected: a subtree whose
*size* changed escalates to its flex container's relayout (taffy handles
via dirty propagation).

**Accept:** `layout_10k_dirty_subtree`-style bench on the live pump path
(new bench `pump_one_of_10k`): one leaf text change relayouts ≪ tree;
bounds identical to full compute (oracle extension: `assert_layout_
coherent` — relayout result ≡ fresh compute, added to
`assert_view_coherent`).

## A.5 — Only-affected restyle + per-node style memo (M, after A.2)

- Memoize rule resolution by (role, id, classes, states, sheet-gen):
  most nodes share a handful of keys → O(distinct keys) per rebuild.
- State flips (hover/focus/pressed) become a **restyle-only** path: no
  scope re-runs (A.1), no relayout (unless A.2 state-part layout rules
  exist on the node), just re-resolve styles for the flipped nodes +
  repaint their damage. This finally makes pointer motion O(2 nodes).

**Accept:** hover storm over the gallery: `ui.lastChange` reports
restyle/patch (not rebuild) per move; style-resolution counter shows
memo hits ≫ misses; suite green.

## Order & wave fit

A.1 (now) → A.2 → A.3.1–A.3.5 → A.4 → A.5. C.1/C.2/T.1–T.4/R.4 run in
parallel (independent of the pipeline). After A.4, revisit
`plan-rendering-performance.md` R5 fragment splicing (retained DL per
scope span) — it becomes natural once spans exist.
