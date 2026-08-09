# Plan: make the incremental path actually incremental (CP-series)

*Design + build plan, 2026-08-05. Supersedes `docs/plan-node-cost.md` (N-series,
falsified). Evidence: `docs/results-node-cost-n0.md`. Instruments:
`benches/benches/nodecost.rs`.*

> **Origin.** The N-series assumed per-node lowering dominated a changed frame and
> was allocation-bound, and scoped an XL retained-graph phase around that. N0 was
> built to make the claim falsifiable and falsified it: no phase exceeds ~25 %,
> allocation is ~10 %, and — the finding that reorganised everything — **the
> memoized path costs more than the full rebuild it replaces.**

---

## Thesis

Lumen already has a fine-grained incremental path. It does not pay off. On the
exact shape the F-series tells authors to write, it is a **net pessimization**:

| | frame | nodes | dirty | allocs |
|---|---|---|---|---|
| `text_list_changed_frame` | 776 µs | 500 | **500/500** | 2 952 |
| `text_list_scoped_changed_frame` | **1 114 µs** | 500 | **1/500** | **5 459** |

Rebuilding everything is **1.44× faster** and allocates **1.85× less** than
rebuilding one row and reusing 499 memo hits.

That is not an architecture problem. Reading `copy_span`/`copy_node`
(`app.rs:2731–2850`) it is a straightforward cost problem with three named
sources, all fixable without a retained node graph. **This plan fixes the
incremental path before considering anything that builds on top of it** — because
every architectural phase the N-series proposed (retained graph, texture caching)
sits downstream of a mechanism that currently loses to doing nothing.

The prize is well defined: bringing the scoped path merely level with the flat
path is **1.44×**, larger than the N-series' entire S-plus phase delivered at its
own best case (1.14× by Amdahl), and it accrues on mobile at the measured ≥2.2×
multiplier.

---

## Foundational invariant (do not violate)

Inherited unchanged — every phase is gated by all three:

```
incremental(state₀, [w₁..wₙ]) == rebuild_fresh(stateₙ)     (F0)
damaged_frame                 == full_repaint              (R0)
semantics_json                 byte-identical              (ADR-009 / F4)
```

Every phase below is a **representation or bookkeeping change**. None may alter
the display list, the goldens, or the agent tree. That is what makes a change to
the copy path safe to attempt at all.

---

## The three measured cost sources

### (a) `copy_span`'s nested-span scan is O(scopes² × span)

```rust
// app.rs:2754 — runs once per memo-hit scope, iterating ALL spans
let nested: Vec<(IdHash, SpanRec)> = self.prev_spans.iter()
    .filter(|(k, r)| **k != key && prev_nodes.contains(&r.root))
```

`prev_spans.iter()` is O(S) per call, called once per copied span ⇒ O(S²), and
the inner `prev_nodes.contains` is a linear `Vec` scan ⇒ O(S² · C). Plus a
`prev_nodes: Vec` and a `stack: Vec` allocated per span purely to feed it.

Measured signature (600 nodes fixed, one scope dirty, only scope count varying):

| scopes | frame | marginal µs per added scope |
|---|---|---|
| 10 | 1 028 µs | — |
| **50** | **804 µs** ← minimum | — |
| 100 | 864 µs | 1.20 |
| 200 | 1 041 µs | 1.77 |
| 300 | 1 382 µs | **3.41** |

Rising marginal cost is the quadratic fingerprint. **Past ~50 scopes, adding
granularity makes the frame slower** — directly contradicting the F-series'
authoring guidance.

### (b) `copy_node` does 8 hash-map operations per copied node

Per node (`app.rs:2799–2848`), all because the fresh tree mints a new
`NodeIndex` each pump so every side table must be re-keyed:

```
prev_meta.remove          → meta.insert
prev_node_style.remove    → node_style.insert
prev_node_computed.remove → node_computed.insert
prev_layout_style.remove  → node_layout_style.insert
```

plus `root_map.insert`, a `LayoutStyle::clone()`, and a **fresh taffy node**.
That is the whole reason a memo hit is not cheap: it skips the closure and the
cascade, and then pays nearly everything else anyway.

### (c) SipHash on every one of those operations

A whole-file hasher swap (measured, then reverted — §6 of the results doc) buys
**9–14 %** with no structural change at all.

---

# Phase CP0 — Fix the gate before changing anything *(S — prerequisite)*

The gate cannot currently detect whether any later phase worked.

## Current state
- `perf_gate.sh` was **red as committed** (two benches had no artifacts). Fixed
  incidentally by the N0 run; the *model* is still wrong.
- Budgets are absolute nanoseconds baked from one dev box.
- The N-series proposed a ±15 % band; measured run-to-run noise is **±2.2 %**, so
  three 14 % regressions compound to 1.48× with a green build.
- The script's own header comment claims to use criterion's change detection. It
  never reads it.

## Steps (each independently green)
- **CP0.1 — Ratio gates.** The acceptance criteria in this plan are *ratios*, and
  ratios survive a slow CI box where absolute nanoseconds do not. Add:
  - `text_list_scoped_changed_frame / text_list_changed_frame` — **today 1.44**
  - `scope_scaling/300 ÷ scope_scaling/50` — **today 1.72**
  - `lower linearity`: `scope_scaling/100 ÷ scope_scaling/50 ≤ 1.15`
- **CP0.2 — Noise-aware regression detection.** Read criterion's
  `change/estimates.json` (point estimate + CI) against a committed baseline;
  threshold ~3σ ≈ **5–7 %**, not 15 %.
- **CP0.3 — Keep the semantic ceilings.** The existing `< 2 ms` / `< 8.33 ms`
  budgets derive from the *frame budget*, not from a measurement — they are
  machine-portable in intent. Do **not** convert them to measured+15 %.
- **CP0.4 — Machine-readable baselines.** `benches/baselines/<machine-id>.json`
  that the gate *reads*, with `--update-baseline`. Not a comment block: comments
  are unverified and rot exactly like the 2026-07-03 phase table this whole
  effort exists to correct.
- **CP0.5 — Absolute-value gating only on a pinned runner**; general CI runs
  correctness + ratios.

*Acceptance:* `perf_gate.sh` is green on a clean tree, fails on a synthetic 8 %
regression, and passes unchanged on a 2× slower machine (test by running it under
`taskset` on one core).

---

# Phase CP1 — Kill the O(scopes²) nested-span scan *(S — highest value/risk ratio)*

## Steps (each independently green)
- **CP1.1 — Reverse index instead of a scan.** When `prev_spans` is taken at the
  start of a rebuild, build `prev_span_by_root: HashMap<NodeIndex, IdHash>` once —
  O(S) total, not O(S) per span. `copy_span` then never iterates `prev_spans`.
- **CP1.2 — Remap inline during the copy walk.** `copy_node` already visits every
  node in the span. When it visits `prev`, look up `prev_span_by_root` in O(1);
  on a hit, write the remapped `SpanRec` immediately. This deletes the `nested`
  `Vec`, the `prev_nodes` `Vec`, and the `contains` scan outright.
- **CP1.3 — Keep the pre-validation, cheaply.** The existing bail (`return None`
  if any node lost its retained work) must survive. Fold the two `contains_key`
  checks into the same walk rather than a separate pre-pass, and bail by
  unwinding the partial copy — or keep a cheap count check, whichever the F0
  oracle proves equivalent.

*Acceptance:* `scope_scaling/300 ÷ scope_scaling/50` drops from **1.72 → ≤ 1.05**
(adding scopes at constant node count must stop costing); the U-curve's right arm
flattens; F0 + goldens + `introspection_f4.rs` unchanged.

---

# Phase CP2 — Cut `copy_node`'s per-node work *(M)*

## Steps (each independently green)
- **CP2.1 — Collapse the four side tables into one.** `node_style`,
  `node_computed` and `node_layout_style` move into the struct `meta` already
  holds (or a sibling `NodeAux`), so the copy path does **1 remove + 1 insert**
  instead of 4 + 4. This is the one piece of the N-series' N1 that survives — but
  it survives on *this* evidence (measured copy-path cost), not on the falsified
  "lowering is 58 % and allocation-bound" thesis. Scope it to the copy path's
  needs and re-measure before extending it.
- **CP2.2 — Stop cloning `LayoutStyle` per node.** `app.rs:2841` clones so the
  map and taffy can each have one. Store `Rc<LayoutStyle>` in the map and have
  `lumen-layout` accept `&LayoutStyle` — a small signature change in
  `lumen-layout/src/tree.rs`, no behaviour change.
- **CP2.3 — Measure the taffy-node cost in isolation** before trying to remove
  it. A fresh taffy node per copied node is the one cost that genuinely needs a
  retained `LayoutTree`, i.e. the expensive direction. Get the number first:
  if it is <5 % it is not worth the retention machinery, and that closes out the
  N3 question on evidence.

*Acceptance:* `text_list_scoped_changed_frame / text_list_changed_frame` drops
from **1.44 → < 0.5** (a 1-of-500 change must not cost half a full rebuild);
allocations for the scoped frame drop below the flat frame's 2 952.

---

# Phase CP3 — Hasher swap *(S, gated on an audit)*

Measured 9–14 % across every bench, via a ~30-line type alias with no new
dependency (ADR-003 clean).

- **CP3.1 — Audit iteration-order dependence.** A different hasher changes
  `HashMap` iteration order. Anything that iterates a `NodeIndex`-keyed map and
  produces output must be order-independent. `serde_json::Map` is a `BTreeMap`
  (no `preserve_order` feature), so `get_styles` already emits sorted keys — but
  this must be *verified*, not assumed, across semantics, styles and the display
  list.
- **CP3.2 — Land the alias** once the audit is clean, with the full golden suite
  as the gate.
- **CP3.3 — Do not extend it to `lumen-core`'s identity hashing.** `IdHasher` is
  pinned deliberately (ADR-021: snapshots and goldens depend on it, and
  `DefaultHasher` was rejected because SipHash is unstable across releases).

*Acceptance:* 9–14 % improvement reproduced; every golden and the semantics JSON
byte-identical.

---

# Phase CP4 — Real ARM measurement *(S, but needs hardware)*

Mobile is a first-class target, so this gates the remaining architecture
questions rather than being a footnote.

## Current state
`nodecost` cross-compiles and runs on the `lumen34` AVD (recipe in the results
doc). It measures **2.18–2.52×** the desktop numbers — but that AVD is **x86_64
under KVM on the dev host**, so that ratio is virtualization, 4-core scheduling
and bionic. **It is not an ARM CPU gap, and it must not be quoted as one.**

## Steps
- **CP4.1 — Run `nodecost` + `perf` on a real ARM device** (`cargo ndk -t
  arm64-v8a`, push, `--bench`). Record the device SoC alongside the numbers.
- **CP4.2 — Recompute the budget table** at the measured ARM ratio against both
  16.67 ms and 8.33 ms.
- **CP4.3 — Add an ARM baseline file** under CP0.4's scheme, so mobile
  regressions are gateable rather than anecdotal.

*Acceptance:* a committed ARM baseline, and the frame-budget table in the results
doc replaced with measured numbers instead of the current 2–3× extrapolation.

---

# Phase CP5 — Re-measure, then decide what survives *(gate, not work)*

After CP1–CP4, re-run the full suite on desktop **and** ARM and answer, in
writing, with numbers:

1. Is the scoped path now cheaper than the flat path? By how much?
2. What is the residual per-node cost, and what is it made of?
3. Does anything from the retired N-series now justify itself — SoA side tables
   beyond CP2.1, the retained node graph, subtree texture caching?
4. Is R4 / the multi-`TaffyTree` split still correctly parked? (Desktop says yes:
   79 µs of a 776 µs frame, 407 µs for 10 000 dirty nodes. ARM may disagree.)

**Explicitly permitted outcome: stop.** If CP1–CP3 bring the incremental path
under the flat path and the ARM budget is comfortable, the correct action is to
record that and do nothing further. The N-series' failure was committing to an XL
phase before the cheap measurements were taken.

---

## Non-goals, with the numbers that retired them

| retired | evidence |
|---|---|
| **N1 SoA refactor** (as scoped) | "allocation-bound" false — allocation is ~10 % of frame. CP2.1 keeps the useful sliver. |
| **N3 retained node graph (XL)** | ~~Marginal 1.6 pp of a 60 Hz frame at 500 nodes, 0.19 pp at 60. Also blocked on decoupling semantics ids from arena slots.~~ **Both grounds void (CP5, 2026-08-09):** the 1.6 pp figure is on the campaign's quarantine list with no derivation, and Phase 1 removed the semantics-id blocker — `SemanticsNode` now takes a `NodeHandle` (`nx-<hex>`) and `conformance.rs` asserts a round-trip property, not a literal. Still not built: measured ceiling is ~1.6× on the memoized path and **zero** on the flat path. Re-scoped as CP5.1 (prototype + measure, ship nothing). `docs/cp5-gate-decision.md`. |
| **N3.4 / A.4 incremental layout** | Formally superseded 2026-07-10 (`plan-retained-pipeline.md:123`) and re-opened by mistake. `relayout_subtree` also pins the subtree to its existing box and never propagates a size delta — wiring it in as-is is a layout-corruption bug. |
| **N4 subtree texture caching** | Conditional on N3; `lumen-render` has no concept of a scope, `KeepAlive` destroys layer textures by design, and it makes the display list backend-dependent. |
| **R4 multi-`TaffyTree` split** | 79 µs of a 776 µs frame. Stays parked (revisit only if CP4.2 says otherwise). |

## ADR impact

None. Every phase is representation or bookkeeping. CP3 explicitly does **not**
touch ADR-021's `IdHasher`.

## ADR-003 / determinism

No new dependencies. CP3's hasher is ~30 lines in-tree. The display list, the
goldens and the semantics JSON are contracts every phase must leave byte-identical.

## Sequencing

```
CP0 (gate) ── CP1 ── CP2 ──┬── CP5 (decide)
                CP3 ───────┤
                CP4 (ARM) ─┘
```

CP0 first — without ratio gates none of the later acceptance criteria are
checkable. CP1 before CP2 (CP1 is smaller and removes a term that would otherwise
confound CP2's measurement). CP3 and CP4 are independent of both.

| phase | size | expected |
|---|---|---|
| CP0 | S | none (makes the rest measurable) |
| CP1 | S | scope-scaling ratio 1.72 → ≤1.05 |
| CP2 | M | scoped/flat ratio 1.44 → <0.5 |
| CP3 | S | 9–14 % across the board |
| CP4 | S | the number that decides everything downstream |
| CP5 | — | a written decision, possibly "stop" |
