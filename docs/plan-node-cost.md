# Plan: closing the per-node cost gap (N-series)

*Design + build plan, 2026-08-04. Companion to `plan-rendering-performance.md`
(R-series: the paint/GPU seam), `plan-fine-grained-view.md` (F-series: the
reactive build), and `plan-remediation-2026-07.md` (A.3/A.4, which N3 completes).*

> **Origin.** A 2026-08 comparison against Makepad's cost model asked "what would
> Lumen have to change to render as cheaply?" Profiling the answer showed the gap
> is **not** where the earlier plans assumed. It is not layout (taffy is ~8 % of a
> changed frame) and, since the R5 glyph-run slice, it is no longer display-list
> emission. It is **per-node lowering and the side tables that hang off it**.

> # ⚠️ N0 RAN 2026-08-05 — THE THESIS ABOVE IS FALSIFIED. PHASES N1–N5 ARE NOT VALID AS WRITTEN.
>
> A performance review found the phase table below unfalsifiable with the
> instruments in the repo (its rows summed to 994 µs against a 773 µs measured
> frame — parts exceeding the whole by 29 %, with the raster row silently
> dropped). N0 was built and run to settle it (`benches/benches/nodecost.rs`).
> **Every headline claim in this plan failed.** Do not start N1 on the basis of
> the Thesis; read `docs/results-node-cost-n0.md` first.
>
> | claim | measured |
> |---|---|
> | lowering dominates (~58 % of frame) | **no** — no phase exceeds ~25 %; the cost is broadly distributed |
> | lowering is allocation-bound | **no** — 2 952 allocs/frame × 25 ns ≈ 74 µs of 776 µs ≈ **10 %** |
> | scope memoization gives O(changed) | **no** — the memoized 1-of-500 frame is **1.44× slower** than the all-dirty rebuild (1 118 µs vs 776 µs) and allocates **1.85× more** |
> | finer `cx.scope` granularity helps | **no** — U-curve; past ~50 scopes at fixed node count, cost *rises*, marginal cost per scope 1.24 → 2.02 → 3.36 µs (quadratic signature, the `copy_span` term) |
> | N1's SoA refactor is the lever | **partly** — a hasher swap alone (no SoA, no classification, no generation stamps) buys **9–14 %** |
>
> **Consequence:** the biggest available win is not in this plan at all. It is
> making the *incremental* path stop costing more than the full rebuild.

---

## Measured baseline (this box, 2026-08-04, release)

Criterion, `cargo bench -p lumen-benches --bench perf`:

| bench | measured | budget |
|---|---|---|
| `idle_frame` | **42.9 ns** | < 2 ms |
| `layout_10k_dirty_subtree` | **382 µs** | < 2 ms |
| `vlist_1m_scroll` | **680 µs** | < 8.33 ms |
| `text_list_changed_frame` (500 nodes, root read ⇒ full rebuild) | **759 µs** | *(ungated)* |

Changed-frame phase breakdown at 500 text nodes (from
`plan-rendering-performance.md` §R5 profiling, updated for the landed R5 slice):

| phase | cost | incremental today? |
|---|---|---|
| build closure (F1-memoized) | 42 µs | ✅ |
| **`build_node` — lower Element → tree + side tables + measure** | **444 µs** | ⚠️ partial (A.3.2 copy-forward) |
| **`build_display_list`** | **304 µs** (was 15.1 ms pre-slice) | ❌ O(tree) |
| **semantics + dep_index** | **125 µs** | ❌ O(tree) |
| layout (taffy) | 79 µs | ❌ but cheap |
| `compute_styles` | ~0 | ✅ (A.5 memo) |

**Reading:** lowering dominates at ~0.9 µs/node, which is allocation-bound, not
compute-bound. Everything in this plan follows from that one number.

---

## Thesis

Makepad is cheap because its widgets are **retained structs** — `handle_event`
mutates them in place and `draw_walk` re-emits from them. There is no lowering
step because there is nothing to lower *into*: the widget **is** the node.

Lumen reconstructs `Tree` + `LayoutTree` + nine `HashMap<NodeIndex, _>` side
tables from a transient `Element` tree on every structural change, then spends
further effort copying unchanged spans forward (A.3.2) to recover part of what the
reconstruction cost. The fix is not to imitate Makepad's authoring model — it is
to stop reconstructing.

Three of the five phases below are already designed elsewhere (A.3, A.4, R5);
this plan sequences them behind two cheap, independent wins and adds the one
genuinely new capability (N4).

**Explicit non-goal: R4 / the multi-`TaffyTree` split.** Measured at 79 µs for a
500-node changed frame and 382 µs to relayout 10 000 dirty nodes. It stays parked
per ADR-R1. These numbers are recorded here so the question is not re-litigated.

---

## Foundational invariant (do not violate)

Inherited unchanged from R0 and F0 — every phase below is gated by both:

```
incremental(state₀, [w₁..wₙ])  ==  rebuild_fresh(stateₙ)      (F0)
damaged_frame                  ==  full_repaint               (R0)
```

Plus one addition this plan introduces, because it touches the observability
projection directly:

```
semantics_json(retained)  ==  semantics_json(rebuild_fresh)   (N3)
```

byte-for-byte. The agent tree is a public contract (ADR-009, F4 conformance in
`introspection_f4.rs`); a retained graph that drifts from a fresh rebuild is a
correctness bug, not a perf trade.

---

# Phase N0 — Per-phase profiler + budget gate *(do this first; it gates N1–N5)*

Every later phase claims a specific number. Today those numbers come from an
ad-hoc 2026-07-03 profiling session, not from anything standing. N0 makes the
claims falsifiable.

## Current state
- `pump()` times total wall clock into `FrameStats` (`app.rs:827`, for `app.perf`).
- No per-phase breakdown, and no bench isolating lowering, DL emission, or
  semantics.

## Steps (each independently green)
- **N0.1** Extend `FrameStats` with per-phase nanoseconds: `build`, `lower`,
  `style`, `layout`, `dl_emit`, `semantics`, `raster`. Diagnostic-only — it must
  never feed rendering, preserving `pump`'s pure-function contract (same rule the
  existing `pump_t0` follows).
- **N0.2** Add three isolating benches: `lower_500_nodes`, `dl_emit_500_nodes`,
  `semantics_500_nodes`. Each drives one phase against a fixed 500-node tree.
- **N0.3** Add a `text_list_changed_frame` budget to `scripts/perf_gate.sh`
  (currently ungated) plus budgets for the three new benches, set at **today's
  measured value + 15 %** so any regression trips before the next phase lands.
- **N0.4** Record the baseline table above into the gate script's comment block so
  the numbers travel with the code.
- **N0.5 — Gate memory, not only time.** Every phase below trades RAM or VRAM for
  speed, and several are *monotonic* (caches that grow and never shrink). Report
  **peak RSS** and **VRAM high-water** alongside the timings, and give both a budget
  in `perf_gate.sh`. Without this, N1–N5 can each regress memory invisibly — the
  same blind spot that let the per-frame `create_buffer_init` churn (N2) survive,
  because the headless benches never saw it.

*Acceptance:* `cargo bench` reports a per-phase breakdown **plus peak RSS and VRAM**;
`perf_gate.sh` fails on a ≥15 % regression in any timing **or memory** budget; the
baseline in this doc is reproducible.

---

# Phase N1 — Collapse the per-node side tables to SoA *(S — biggest win per unit of risk)*

## Current state
`app.rs` holds **nine** `HashMap<NodeIndex, _>` side tables, all rebuilt per pump:

```
meta               : HashMap<NodeIndex, NodeMeta>            (app.rs:556)
node_ink           : HashMap<NodeIndex, kurbo::Rect>              :561
node_caret         : HashMap<NodeIndex, kurbo::Rect>              :564
node_text_metrics  : HashMap<NodeIndex, TextMetrics>              :567
node_style         : HashMap<NodeIndex, Style>                    :581
node_computed      : HashMap<NodeIndex, HashMap<String, Computed>> :582
node_layout_style  : HashMap<NodeIndex, LayoutStyle>              :598
prev_meta / prev_node_style / prev_node_computed                  :593–595
```

Two compounding costs:

1. **Hashing + per-entry allocation** on every insert, × 9 tables × N nodes.
   `node_computed` is the worst offender: a `String`-keyed `HashMap` **per node**.
2. **`NodeMeta` is fat** (`app.rs:454`): `String` label, `Vec<String>` classes,
   `Vec<Action>`, `Vec<SemState>`, `NodeDeps`, and **thirteen** `Option<Handler>`
   fields inline. Most nodes have zero handlers and no classes, so this is ~100+
   bytes of mostly-`None` per node.

`NodeIndex` already carries what's needed to fix this — it is
`{ index: u32, generation: u32 }` and `index()` is documented verbatim as *"The
dense slot index. **Use to address SoA arrays**."* The tree is SoA; the app-level
side tables simply never followed it.

## Steps (each independently green)
- **N1.1 — Classify the tables dense vs sparse *before* converting any of them.**
  A `Vec<Option<T>>` allocates a slot for **every** node index, so it is a memory
  *regression* for a sparse table. `node_caret` typically holds one entry (the
  focused text field); as a `Vec<Option<Rect>>` over a 10 000-node tree that is
  ~400 KB standing in for one live value. Convert only the tables that are
  genuinely dense (`meta`, `node_style`, `node_computed`, `node_layout_style`).
  Leave sparse tables (`node_caret`, and `node_text_metrics` on non-text-heavy
  trees) as maps, or move them to a sparse-set / sorted `Vec<(NodeIndex, T)>`.
  Measure occupancy in N0 first — do not guess which is which.
- **N1.2 — `Vec` conversion for the dense tables.** `Vec<Option<T>>` (or a dense
  `Vec<T>` + validity bitset) indexed by `NodeIndex::index()`. Nodes allocate
  preorder into a fresh tree, so indices are dense and contiguous by construction
  (per the `build_node` span comment). **Generation check:** keep a parallel
  `Vec<u32>` of generations and assert the stamp matches on read, so a stale
  `NodeIndex` faults loudly instead of aliasing.
  **Allocation reuse is a deliberate RAM-for-time trade.** Today
  `std::mem::take(&mut self.meta)` (`app.rs:2496`) moves the table into `prev_*` and
  builds a fresh one, so the allocator reclaims each cycle. Reusing capacity
  (`clear()`, not re-`new()`) removes that churn but pins steady-state RSS at the
  **high-water** tree size for the session. Acceptable only with N0.5's memory gate
  and a shrink trigger: if occupancy stays below 25 % for N consecutive pumps,
  `shrink_to_fit`.
- **N1.3 — Interned strings.** `label: SmolStr`, `value: Option<SmolStr>`,
  `classes: Rc<[SmolStr]>`. `smol_str 0.3` is already a workspace dependency and
  ADR-003-whitelisted — no escalation. Strictly reduces memory: `SmolStr` stores
  up to 22 bytes inline, so short labels stop heap-allocating entirely.
- **N1.4 — Shared empty slices.** `actions: Rc<[Action]>`, `states: Rc<[SemState]>`
  with a shared static empty. Most nodes have neither.
- **N1.5 — Box the handler bundle.** Move the thirteen `Option<Handler>` fields
  into `struct NodeHandlers { … }` behind `handlers: Option<Box<NodeHandlers>>`.
  `Handler` is `Rc<dyn Fn(&Runtime)>` — a 16-byte fat pointer — so thirteen inline
  is **208 bytes per node**, almost always `None`. A handler-free node becomes one
  null pointer; a node *with* handlers pays 8 bytes plus one boxed allocation.
  Large net reduction, since handler-bearing nodes are a small minority.
- **N1.6 — `node_computed` keys.** The heaviest table by far: today a `String`-keyed
  `HashMap` **per node**, i.e. roughly one outer entry + one inner map allocation +
  one `String` allocation per property, per node. Replace with a sorted
  `Vec<(PropId, Computed)>`, and share it as `Rc<[…]>` across nodes with an
  identical computed set — very common, since every row of a list resolves the same
  cascade. Property names are a closed set; intern them to a `u16` `PropId` at parse
  time in `lumen-style`. Expected to be the single biggest memory reduction in the
  plan.
- **N1.7 — Re-measure** against N0.2's `lower_500_nodes` **and N0.5's RSS budget**.

## Guards
F0 `assert_view_coherent` + the full golden suite + `introspection_f4.rs`
conformance. The semantics JSON must be **byte-identical** before and after — this
phase changes representation only.

*Acceptance:* `lower_500_nodes` improves ≥2×; no golden or conformance diff; the
`text_list_changed_frame` gate from N0.3 moves down, not up.

---

# Phase N2 — Persistent GPU buffers *(S/M — independent of N1/N3)*

## Current state
`flush_rects` (`gpu.rs:257`) and `flush_paths` (`gpu.rs:275`) call
`device.create_buffer_init` on **every flush, every frame**; there are 13
buffer-creation sites in `gpu.rs`. Each `LayerDraw` variant owns its `wgpu::Buffer`.
Makepad writes into persistent instance buffers instead.

This cost is invisible to the headless benches, which is why it has survived —
it only shows on the windowed path.

## Steps (each independently green)
- **N2.1 — `BufferPool`.** One growable persistent `wgpu::Buffer` per usage class
  (vertex / index / instance). Frame start resets a bump offset; each flush
  `queue.write_buffer`s at the current offset and advances.
- **N2.2 — `LayerDraw` holds offsets.** Replace the owned `buf: wgpu::Buffer`
  fields with `(offset: u64, count: u32)` into the pool. Respect
  `wgpu::COPY_BUFFER_ALIGNMENT` (4) when advancing, and 256-byte alignment for
  anything bound with a dynamic offset.
- **N2.3 — Grow policy with decay.** Double on overflow; reallocate at frame
  boundaries only, never mid-pass. **Do not adopt a never-shrink policy:** the pool
  would be pinned at the session's worst frame forever, so one complex canvas or SVG
  tessellation permanently doubles resident VRAM. Track a high-water mark over a
  sliding window (~120 frames) and shrink to `2 × window_peak` when the pool has
  been under 25 % utilized for the whole window. Log pool size at
  `app.perf` so growth is observable.
- **N2.4 — Apply across all 13 sites** — rects, paths, glyphs, gradients, images,
  composites.
- **N2.5 — Windowed benchmark.** This box has an RTX 4070 + `DISPLAY=:0`, so
  measure a real presented frame, not an offscreen one.

## Guards
R0 `cpu_vs_gpu` differential and `damage_equivalence` — output must stay within
the existing tolerance. Goldens untouched: this changes *where bytes live*, never
what is drawn.

*Acceptance:* zero `create_buffer_init` calls in the steady-state frame path
(assert via a debug counter); windowed frame time improves; R0 differentials pass.

---

# Phase N3 — Retained node graph *(XL — the real fix; completes A.3.3 + A.4)*

## Current state
- **A.3.1 landed** — scope roots carry `scope_key`; `build_node` records the node
  span (`prev_spans`).
- **A.3.2 landed** — a memo-hit returns an `Rc` stub (`el.shared`); `copy_span`
  copies an unchanged span forward when the outside-context hash matches.
- **Still per-pump:** the `Tree`, the `LayoutTree`, and all nine side tables are
  *reconstructed*. Copy-forward re-lowers into a **fresh** tree; it avoids re-running
  the closure, not the allocation.
- **A.4 blocked on this** — `relayout_subtree` exists but is test-only, because
  there is no retained tree to relayout.

## Target
Retain `Tree` / `LayoutTree` / side tables / semantics / dep-index across pumps.
On rebuild, splice only the spans belonging to dirty `cx.scope`s.

## The two hard problems (spell them out before starting)

1. **Identity across pumps.** `NodeIndex` is build-scoped today: nodes allocate
   preorder into a fresh tree, so index 7 means nothing across two pumps. A
   retained splice needs identity that survives — key by `(scope_key,
   ordinal-within-span)` and let the generation stamp catch staleness.

2. **Splicing a span of different length.** Replacing `[start, end)` with a
   newly-lowered span of a different size either renumbers everything after it
   (O(tree), defeating the purpose) or requires an arena that tolerates gaps.
   Two viable designs, pick with a spike:
   - *(a) Gap arena* — spans get slack; renumber only on overflow. Simple, wastes
     memory, amortizes well for stable trees.
   - *(b) Indirection layer* — a stable `NodeId → NodeIndex` map, with the dense
     arrays compacted lazily. Costs one indirection on every node access, which
     partly gives back N1's win. Measure before committing.

## Steps (each independently green)
- **N3.1 — Spike the two splice designs** against `lower_500_nodes` with a
  synthetic "one row changes in 500" workload. **Score both on RSS as well as
  time** — design (a) buys speed with slack, and 1.5–2× on the node arrays is a
  plausible cost; design (b) adds a stable-id indirection (~8–16 B/node) and leaves
  dead slots until compaction. Write up the choice here before building.
  Note the offsetting *reduction*: a retained graph knows what it spliced, so the
  `prev_meta` / `prev_node_style` / `prev_node_computed` mirrors (`app.rs:593–595`)
  — a full second generation of the three heaviest tables — may become unnecessary.
  Net direction is genuinely unclear until measured; do not assume it is a
  regression *or* a win.
- **N3.1b — Retention policy.** A retained graph holds the peak tree ever built,
  not the current one. Navigating away from a large view must release it: compact
  on scope eviction (`evict_scope` already tracks ownership) and shrink the arenas
  when live-node count drops below half of capacity.
- **N3.2 — Stable node identity.** `(scope_key, ordinal)` keying + generation
  validation. No behavior change; proven by asserting the retained tree's identity
  map matches a fresh rebuild's preorder.
- **N3.3 — Splice primitive.** Implement the chosen design behind a runtime flag
  `retained_graph: bool`, default **off**.
- **N3.4 — Retain `LayoutTree` in step.** taffy nodes created/removed to match the
  splice; **wire `relayout_subtree` into the live pump** — this is A.4, unblocked.
- **N3.5 — Retain semantics + dep-index.** Recompute only spliced spans. This is
  where the 125 µs goes to O(changed) and where the byte-identical invariant bites
  hardest.
- **N3.6 — Handler currency.** ADR-013: handlers are recreated when a scope re-runs
  and reused from the F1 memo when skipped. A *retained* node now holds its handler
  `Rc` across pumps — re-verify the F2 handler-currency check and the
  `stable_handler!` `Copy` assertion still hold, and that `input.invokeAction` (F4)
  never actuates a stale handler.
- **N3.7 — Dual-path soak.** With the flag on, run *both* paths in CI for a release
  cycle and assert equality via the F0 oracle plus the new semantics-JSON invariant.
  Extend the existing 80-round mixed fuzz to alternate the flag.
- **N3.8 — Flip the default** once the soak is clean; keep the flag as an escape
  hatch for one release.

*Acceptance:* `lower_500_nodes` and `semantics_500_nodes` both become O(changed);
a per-row-scoped 500-row list re-lowers one row on a one-field write;
`retained == rebuild_fresh` structurally **and** in semantics JSON across the fuzz.

---

# Phase N4 — Automatic subtree texture caching *(M — conditional on N0 data; depends on N3)*

Makepad exposes `texture_caching: true` as a manual annotation: render a subtree
to an offscreen texture and blit it while it is stable. It has to be manual there
because Makepad has no dependency tracking to derive it from.

**Lumen can derive it.** A scope whose `ReadSet::is_current()` holds and whose
layout box is unchanged is *provably* stable this frame — that is precisely what
F1 already computes. This is the one place in this plan where Lumen can be
cheaper than Makepad rather than merely equal.

## Current state
`LayerDraw::Composite` and the composite pipeline already exist (used for opacity
and clip layers), so the blit machinery is present. Nothing derives caching.

## Steps (each independently green)
- **N4.1 — Eligibility predicate.** A scope is cacheable when *all* hold: its
  `ReadSet::is_current()`; its layout box is unchanged in **both** size and origin
  (the R5 origin-shift problem applies verbatim); it contains no overlay descendant
  (overlays escape ancestor clips and break subtree contiguity — R5 sub-problem 2);
  it is not impure (`dyn_text`/`dyn_bg`/`dyn_classes`/`Canvas`/`Custom`/any
  frame-request — `build_node` already tracks this as `impure_seen`); and its pixel
  area exceeds a threshold below which redrawing beats blitting.
- **N4.2 — Cache + eviction under a hard VRAM cap.** One offscreen texture per
  cached scope, keyed by `(scope_key, size, scale)`. Invalidate on scope re-run,
  resize, scale change, theme or stylesheet generation bump.
  **This is the largest resource increase in the plan and the budget must be a hard
  cap, not advisory.** A 280×1080 logical sidebar at 2× DPI is 560×2160×4 B ≈
  **4.8 MB** of VRAM; ten cached subtrees is ~48 MB. Set a conservative default
  (suggest 32 MB, configurable), evict LRU by area, and **degrade gracefully to
  direct rendering** when the cap is hit or an allocation fails — never fail a
  frame. Report cache size and hit rate through `app.perf`.
- **N4.3 — Blit path.** Emit a `Composite` of the cached texture in place of the
  subtree's draws.
- **N4.4 — New differential oracle.** R0's `cpu_vs_gpu` does **not** cover this —
  the CPU backend has no offscreen textures. Add `cached_blit_equivalence`: a frame
  rendered with caching enabled must match a full render within the existing R0
  tolerance. Treat this as a first-class new gate, not an afterthought.
- **N4.5 — HiDPI.** Cache at physical px, keyed by scale; a scale change evicts.

## Honest gating
After N3 makes re-emission O(changed), N4's marginal value drops sharply — it only
pays for a subtree that is *expensive*, *stable*, and *not moving*. **Do not build
this speculatively.** Gate it on N0 profiling showing a real workload (a static
sidebar beside a scrolling pane is the canonical one) where it wins.

*Acceptance:* a static sidebar in a scrolling app costs one blit;
`cached_blit_equivalence` passes; disabling the cache changes timing only, never
pixels.

---

# Phase N5 — Full R5 fragment splicing *(L — conditional)*

Inherits `plan-rendering-performance.md` §R5 unchanged. R5.1′ (the origin-relative
glyph-run cache) already landed and captured ~50× — the documented expectation was
that it would capture *essentially all* of R5's benefit, leaving only the marginal
rect/gradient/image share.

**Trigger:** build this only if N0.2's `dl_emit_500_nodes` shows non-text emission
dominating on a real workload. Otherwise R5.1–R5.3 stay documented and unbuilt.

The three known sub-problems (recursive clip bracketing, overlay non-contiguity,
`ImageId` remapping on cross-build reuse) and the origin-shift analysis are already
written up in that plan; they are not restated here.

---

## Sequencing

```
N0 ──┬── N1 ────────────┐
     │                  ├── N3 ── N4 (conditional)
     └── N2 (parallel) ──┘
                        └── N5 (conditional on N0 data)
```

N1 and N2 are independent of each other and of N3, and both are small. Land them
first: they are real wins that also sharpen N0's numbers before the XL phase
starts. N3 is the only phase that needs a design spike before estimation.

| phase | size | expected effect |
|---|---|---|
| N0 | S | none (measurement) |
| N1 | S | lowering ≥2× |
| N2 | S/M | windowed frame; removes per-frame GPU allocation |
| N3 | XL | lowering + semantics → O(changed); unblocks A.4 |
| N4 | M | stable-subtree blit; conditional |
| N5 | L | non-text DL emission → O(changed); conditional |

## Resource budget (cross-cutting — read before starting any phase)

Every phase in this plan buys time with memory, and five of them do it by adding
something that persists across frames. Left to individual judgement that becomes
five ad-hoc growth policies; stated once, it is a single reviewable trade.

| phase | direction | magnitude | why |
|---|---|---|---|
| N0 | ~neutral | a few dozen bytes/frame | per-phase counters on `FrameStats` |
| N1 | **large reduction**, with one trap | `node_computed` dominates the win | interning + `Rc`-sharing + boxing 208 B of mostly-`None` handlers; the trap is `Vec<Option<T>>` over *sparse* tables (N1.1) |
| N2 | **increase (VRAM)** | pool ≈ 2 × peak frame | persistent buffers; bounded by N2.3's decay |
| N3 | **unclear until measured** | arena slack up to ~2 ×, offset by dropping `prev_*` | retained graph vs. today's two generations |
| N4 | **increase (VRAM), largest** | ~4.8 MB per cached sidebar | offscreen textures; bounded by N4.2's hard cap |
| N5 | modest increase | bounded by tree size | retained DL fragments |

Three rules, binding on every phase:

1. **No unbounded cache.** Every retained structure has an explicit cap and an
   eviction path. A cache without a cap is a leak with good intentions.
2. **No monotonic growth.** Anything that grows must be able to shrink — high-water
   decay (N2.3), compaction on eviction (N3.1b), LRU under a cap (N4.2).
3. **Degrade, never fail.** Hitting a memory cap disables the optimization for that
   frame; it never drops a frame or panics.

N0.5 is what makes these enforceable rather than aspirational.

## ADR-003 / determinism

No new dependencies in any phase. `smol_str` (N1.2) is already whitelisted and a
workspace dependency. Every phase is representation-only: the display list, the
goldens, and the semantics JSON are contracts that must come out byte-identical,
which is what makes an XL phase like N3 safe to attempt at all.

## Escalations

- **N3.1** may conclude that neither splice design pays for itself at Lumen's
  typical tree sizes. That is a legitimate outcome — record it and stop at N1+N2.
- **N4.4** introduces a new correctness oracle. If it cannot be made
  deterministic across GPU vendors, N4 does not ship.
