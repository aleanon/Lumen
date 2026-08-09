# The 6× gap to egui: eight falsifications and no hotspot

*2026-08-09. Phase-level instrumentation of `pump()`, then ablation.*

BENCH1 found Lumen ~6× slower than egui in absolute terms at every size (1000
rows: **1732 µs vs 267 µs**), with matching scaling. This is the search for the
constant factor.

## Where the frame goes

1000 rows, 400×800, steady state, temporary `Instant` timers around each phase
(removed after measuring; TSC clocksource, 26.6 ns per `now()+elapsed` pair, so
instrumentation overhead is accounted for and small):

| phase | µs | share |
|---|---:|---:|
| view closure | 65 | 4% |
| **`build_node` (lowering)** | **887** | **59%** |
| layout (taffy) | 136 | 9% |
| paint (display list) | 319 | 21% |
| semantics | 86 | 6% |

**egui's entire frame is 267 µs.** `build_node` alone is 3.3× that.

## Inside build_node: no hotspot

~875 ns per node — and **content-independent**: 1000 *empty containers* still
cost ~740 ns each, with zero text.

| sub-step | µs | share of build |
|---|---:|---:|
| text shaping/measure (cached) | 132 | 15% |
| taffy leaf construction | 88 | 10% |
| `NodeMeta` construction | 64 | 8% |
| `.lss` matching | 10 | 1% |
| tree node allocation | ~0 | ~0% |
| **unattributed, diffuse** | **~450** | **~50%** |

## What was falsified, and how

Each of these was a plausible cause. Each was measured, not argued.

| hypothesis | test | result |
|---|---|---|
| CSS selector matching per node | timed the block | **10 µs (1%)** — skipped entirely with no stylesheet |
| Tree node allocation (11 parallel `Vec` pushes) | timed the call | **~0 µs** |
| Per-frame container regrowth + rehashing | reserved capacity from last frame's count | **no change** (874→879 text; 743→699 empty) |
| `NodeMeta` construction | ablated the insert | **64 µs (8%)** |
| `Element` size (1072 B moves) | padded it to 2144 B | **2–8%** on per-node cost |
| The view closure (1000 `format!`s) | timed it | **65 µs (4%)** |
| Semantics tree | timed it | **86 µs (6%)** |
| Text shaping | timed it | **132 µs (15%)**, already cached |

**Nothing is above 15%.** That is the finding: there is no bug to fix here. The
per-node path is simply long — a retained tree node, a taffy node, a `NodeMeta`,
a display-list contribution and a semantics node, versus egui appending vertices
to a mesh.

## The one place a cache is doing real work

Paint is **319 µs with 1000 distinct strings and 21 µs with one shared string**.
So ~300 µs of paint is glyph-run construction for distinct text — the cost
egui's galley cache avoids by keying on the string. Lumen's run cache works
(that is what the 21 µs shows); it just cannot help when every row differs,
which is the benchmark's shape.

## Why this is not fixable by optimisation

The existing gate reports `scoped_vs_flat = 0.787`: a memoized rebuild with one
dirty row costs **79% of a full rebuild**, not ~0.1%. Memoization avoids
re-running view closures but still re-lowers every node — `copy_span` rebuilds
taffy nodes and re-inserts side-table entries for the copied subtree.

So the per-node lowering cost dominates *both* paths. Halving any single
sub-step above moves the frame by <8%. Reaching egui's 267 µs from 1493 µs of
measured phases requires not doing per-node work at all for unchanged nodes —
which is the retained-tree direction (CP6), declined earlier at 4.46% against
CP2.3's 5% bar. **That decision was made on a narrower question** (persisting the
taffy arena) than the one this measurement raises (retaining the whole lowered
node across frames), and is worth re-running with this number as the input.

## Honest framing of the comparison

The benchmark measures a **full rebuild** on both sides, which is the fairest
available shape — egui is immediate-mode and has no incremental path. But it is
Lumen's worst case, and `scoped_vs_flat` says its best case is only 21% better.
That is the real result: **Lumen has an incremental architecture whose
incremental path costs nearly as much as its full path.** The 6× is the price
of building five per-node structures where egui builds one, paid on every frame
either way.
