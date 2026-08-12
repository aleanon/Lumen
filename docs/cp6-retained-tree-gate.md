# CP6 re-gate, larger version — **STOP, and the reason is not the number**

*Decision, 2026-08-13. Re-run at the owner's request against the bigger scope
CP5.1 identified: retained **tree + side tables + taffy arena**, not the
"persist the arenas" wording the 2026-08-08 gate ruled on.*

## What changed since the last ruling

`docs/cp5-retained-arena-decision.md` stopped **CP6.1** (persist `LayoutTree`)
at 4.46%, under CP2.3's 5% bar — and explicitly left **CP6.2 (persist `Tree`)
open**, saying it "needs its own measurement before its own gate". CP5.1 then
found re-lowering to be 33.8% of a memo-hit frame with taffy only ~18% of that,
which is what prompted this re-gate. This document is CP6.2's gate.

## Q1 — What does full retention actually remove?

Same 500-row one-dirty shape, in-situ brackets, every figure corrected for the
26.6 ns instrumentation pair (two runs, consistent):

| category | µs | share of frame | per node |
|---|---:|---:|---:|
| **tree node + flags/z** | **0.0** | **0.0%** | **~0 ns** |
| index churn — `NodeMeta`/style/computed moves, `root_map` | 72.4 | 13.8% | 145 ns |
| taffy node construction | 33.1 | 6.3% | 66 ns |
| **all three** | **105.5** | **20.2%** | |
| (CP5.1's whole-`copy_span` figure, for reference) | 181.6 | 33.8% | |

**Building the tree is free.** Below the noise floor — consistent with
`docs/six-x-gap-investigation.md`, which timed tree allocation at ~0 µs. So
"persist `Tree`" is worth nothing *for its own cost*.

That is the finding, and it inverts the proposal: the tree matters only as an
**enabler**. Because it is rebuilt each frame, every node gets a new
`NodeIndex`, and that is what forces 145 ns/node of hash-map churn moving side
tables from the old index to the new one. You retain the tree not to save the
tree, but to stop the indices moving.

So the honest range for what full retention removes is **20% (attributed) to
34% (if the whole copy path disappears)** of a memo-hit frame:

```
today                            scoped_vs_flat = 0.648
retention, conservative (20.2%)                   0.517
retention, optimistic  (33.8%)                    0.429
```

By the CP5 gate's own line — *"near 0.49, CP6 has a real case; near 0.787, the
retained graph is dead on measurement"* — this **straddles it**, and by CP2.3's
5% bar it clears comfortably. **On the number alone, this would be a go.**

## Q2 — Who is on the memoized path?

Nobody.

| | count |
|---|---:|
| example crates in the repo | 51 |
| example crates using `cx.scope` or `widgets::keyed` | **0** |
| shipped widgets that memoize their children | **0** |
| callers of `widgets::keyed` outside its own test | **0** |
| `cx.scope` call sites in non-test code | **1** (inside `keyed` itself) |

`VirtualList`, `DataGrid` and the virtual table — the widgets whose whole
purpose is large row counts — call `render(i)` directly. Mercurium's UI crate
has no scopes either.

**CP6 optimizes a path exercised by one test and one benchmark.** The 2.3×
is real and it currently applies to no frame any user renders.

## Q3 — What is the cheaper thing that unlocks it?

Adoption. If the shipped list widgets wrapped their rows in `cx.scope`, every
list-heavy app would land on the memoized path, and the measured
`scoped_vs_flat = 0.648` would start applying to real frames:

| step | memo-hit frame vs a full rebuild | reversible? |
|---|---:|---|
| **adoption alone** (widgets memoize) | **1.54×** | yes |
| adoption + retention | 1.93× – 2.33× | **no — one-way door** |

Adoption is the larger single step, it needs no new machinery, and it is
reversible. Retention is the smaller increment on top and is the campaign's
hard one-way door. Doing them in the other order means paying the irreversible
cost first for a benefit nothing can yet observe.

It also makes the eventual re-gate *honest* in a way this one cannot be: with
real widgets memoizing, the 20–34% would be measured against workloads people
actually run, on tree shapes that are not a flat 500-row column.

## Q4 — What would retention cost?

Not attempted here beyond what is needed to judge the trade, but the shape is
known and it is not small:

* `rebuild_inner` does `let mut tree = Tree::new()` every pump and repopulates
  every side table. Retention replaces from-scratch construction with diff +
  patch, and `NodeIndex` generations become load-bearing across frames rather
  than within one.
* Taffy never frees an unreferenced node, so retained taffy nodes need explicit
  reclamation plus a node-count assertion — flagged in the 2026-08-08 gate and
  still true.
* It opens a stale-retained-work bug class: a reused node keeping a style,
  meta or layout entry that should have changed. `assert_view_coherent`
  (incremental ≡ rebuild-fresh) is the right oracle for exactly this and
  already exists, which lowers the risk but does not remove the work.

## Decision

**STOP on CP6. Not on the measurement — on the absence of a beneficiary.**

The number clears every bar the campaign set. The path it improves has no
users, and the reversible step that would give it users has not been taken.
Committing an XL one-way change to speed up a code path that no shipped widget
and no example uses is the same mistake the N-series is retired for, wearing
better numbers.

**Successor, and it is real work rather than another measurement:**

> **ADOPT — make the shipped list widgets memoize their rows.**
> `VirtualList`, the virtual table and `DataGrid` wrap each row in `cx.scope`
> keyed by row identity. Reversible, no new machinery, and the measured payoff
> is 1.54× on a one-row change. Watch for the scope-key trap ADR-021 records
> (a flat string key resolves to a different, root-level signal — it silently
> no-op'd `scope_memo_one_of_many` once already).

**Re-gate CP6 after adoption**, against list workloads rather than a synthetic
column, and — still outstanding from every previous ruling — **with an ARM
number**. CP4 remains hardware-blocked (`docs/cp4-arm-measurement-blocked.md`),
and per-node cost is the thing most likely to look different there.

## What this does not say

* **Nothing about the egui gap.** BENCH1's workload has no `cx.scope`, so
  neither retention nor adoption moves that ratio by a microsecond. Third time
  this has needed saying in a CP document; it keeps inviting the wrong
  conclusion.
* **Nothing against `cx.scope` itself.** F1 shipped a memoization mechanism that
  works and measures 0.648. The gap is that nothing calls it.
