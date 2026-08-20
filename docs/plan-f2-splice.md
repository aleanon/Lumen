# F2 / A.3.3 — splice-in-place: the retained node graph

*Design + staged plan, 2026-08-20. Revives `plan-retained-pipeline.md` A.3.3,
which was deferred with an explicit trigger: "Revisit with the R-phase benches
if profiles show the shallow walk dominating."*

**The trigger has fired.** PROF1 (`docs/profile-vs-iced-2026-08-19.md`) and the
R1–R6 series measured it directly.

## What is settled before starting

**A.4 (incremental layout) stays skipped — re-tested, not assumed.** The
2026-07-03 decision was made against taffy 0.7, and we now run 0.13, so the
premise was re-measured on a 3000-leaf flex column:

| taffy 0.13 | time |
|---|---:|
| cold solve | 743.6 µs |
| re-solve, nothing dirty | **84.0 µs** |
| re-solve, one leaf dirty | 347.7 µs |
| re-solve, all leaves dirty | 424.4 µs |

taffy 0.13 *does* cache — a fully-clean re-solve is 9× faster than cold, which
is new information. But **one dirty leaf costs 82% of all-dirty**, because a
flex parent must re-run its main-axis pass over every child once any child's
size moves. For a list — the shape this whole campaign measures — incremental
layout buys almost nothing. The skip stands, now for a measured reason rather
than an inherited one.

**So F2's prize is node *re-creation*, not re-solving.**

## The prize, measured

An idle pump at 3000 rows costs **0.1 µs** — Lumen already short-circuits a
frame with nothing dirty. A pump where **one row of 3000** changed costs
**1729 µs**. Essentially all of that is work on the 2999 rows that did not
change.

In the memoized variant — where `cx.scope_with_deps` marks 2999 rows as memo
hits and `FrameStats` reports 2 rebuilt / 2999 copied — the frame is *still*
1820 µs, and the profile says where:

| symbol | share of the memoized frame |
|---|---:|
| `__memmove_avx_unaligned_erms` | 21.0% |
| `build_node` | 9.0% |
| `taffy::compute_preliminary` | 6.7% |
| `LayoutStyle::to_taffy` | **4.2%** |
| `taffy` compute_child_layout | 4.0% |
| `copy_node` | **3.8%** |
| `rebuild_inner` | 3.3% |
| `cached_if_current` | 3.1% |
| `slotmap::try_insert_with_key` | **3.0%** |

The bolded rows are pure re-creation of things that did not change:
`copy_node` allocates a fresh `NodeIndex`, mints a **fresh taffy node**
(`to_taffy` + slotmap insert), and re-keys nine side tables from `prev_*`. The
memmove is dominated by copying taffy's `Style` and `LayoutStyle`/`NodeMeta`
into their new homes.

**"Copy forward" does not reuse a node. It re-materialises one.** F2 is making
that phrase true.

## Target metric

`build_frame/lumen_memoized` at 3000 rows — **1820 µs today**. The plain
variant cannot improve until memo spans exist, which is why PROF1's R4
(automatic memoization) is sequenced *after* this, not before: today it is
worth nothing, because the copy path costs as much as a rebuild.

## Stages

Each stage is gated on `assert_view_coherent` (the F0 oracle — 86 call sites
compare the display list *and* the semantics tree against a fresh rebuild),
the golden corpus, and the full twelve-leg gate.

### F2.1 — reuse taffy nodes for copied spans — **LANDED 2026-08-20, −18.3%**

Retain the `LayoutTree` across frames instead of clearing it, keep a
`NodeIndex → LayoutNode` map for the previous frame, and have `copy_node`
**reuse** the previous taffy node rather than minting one. Re-parenting is
free: `container(style, children)` already re-parents whatever nodes it is
given.

Removes `to_taffy` (4.2%), `slotmap::try_insert_with_key` (3.0%) and their
share of the memmove. Needs a removal path for the taffy nodes of spans that
*were* rebuilt, or the tree grows without bound.

### F2.2 — reuse arena nodes for copied spans

The retained `Tree` keeps its `NodeIndex` for a copied node, so the nine side
tables are not re-keyed and `NodeMeta` is not moved. This is where the memmove
goes.

### F2.3 — stop walking copied spans at all

With F2.1 and F2.2, a copied span is already identical in place; the walk over
it becomes pure bookkeeping. Removing it is what turns O(tree) into O(changed)
and is the actual A.3.3 acceptance.

### F2.4 — incremental semantics and dep index

Both are already lazy (`sem_root` builds on demand, OB2), so this is
invalidation scope rather than new machinery.

## Risks, and what guards each

* **Stale nodes.** A copied span that should have changed. Guarded by the
  coherence oracle, which rebuilds fresh and diffs — the exact failure it was
  built for.
* **Arena leaks.** Nodes freed from a rebuilt span must return to the free
  list, and their taffy counterparts must be removed. Guarded by
  `check_invariants` (which now validates `last_child` too, TG1) and by a node
  count assertion per frame.
* **Identity drift.** `NodeIndex` becoming stable changes what a recycled slot
  means; ADR-021's `NodeHandle` is the structural identity and is unaffected,
  but `fold64` publication to AccessKit must stay stable across a splice.
* **The bisect hatch stays.** `LUMEN_FULL_REBUILD=1` already forces the naive
  path; every stage below keeps it working, so any incoherence can be bisected
  against a known-good rebuild in a live run.

## Measured before implementing (2026-08-20)

`benches-competitive/src/bin/probe_f2_reparent.rs`, `taskset -c 2`, 3000 rows,
min of 30. Each line is one frame's worth of taffy work — node creation plus
`compute_layout`:

| shape | µs | |
|---|---:|---|
| A. clear + re-mint all 3000, compute | 540.8 | **today** |
| B. reuse 3000 nodes, mint the parent, adopt 3000 | 300.1 | **F2.1, real shape** |
| C. compute only, nothing dirty | 85.7 | the floor |
| E. reuse a whole span, parent adopts **1** child | 84.9 | needs a memoized container |

Three things this settles:

1. **The re-parenting order is safe.** Adopting the reused children into the
   new parent *first* and removing the stale parent *second* leaves layout
   correct and the live node count flat across 30 frames (3001 → 3001). The
   ordering hazard the design was working around does not exist in taffy 0.13,
   so `copy_node` can adopt naively and free stale nodes in a later pass.
2. **F2.1 is worth ~240 µs** (A → B), about 13% of the 1820 µs memoized frame.
   Consistent with the profile's `to_taffy` 4.2% + `slotmap` 3.0% +
   `compute_preliminary` 6.7% + `compute_child_layout` 4.0% ≈ 18%.
3. **The other ~215 µs is the adoption itself** (B − C). `vs_iced.rs` scopes
   each row separately and leaves `widgets::column(rows)` outside the memo, so
   the container is re-minted every frame and rewrites 3000 parent pointers.
   Shape E — the 85 µs floor — is only reachable once an *unchanged container*
   can keep its taffy node instead of re-adopting an identical child list.
   That is the real prize, and it belongs to F2.2/F2.3, not F2.1.

Not a target: reusing a *changed* row's node and calling `set_style` on it
measured **650.7 µs** — worse than re-minting the whole tree, because dirty
propagation forces a full re-solve of the flex column on top of the adoption.
Re-lowering a changed span, as the runtime already does, is the faster path.
