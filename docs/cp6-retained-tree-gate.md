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

## Addendum — what ADOPT costs in memory (2026-08-13)

Asked before committing to the successor, and measured rather than reasoned
about: a live-bytes allocator (subtracting on free, unlike `nodecost.rs`'s churn
counter), release build.

`CachedScope` holds an `Rc<Element>`, so memoizing keeps a row's built subtree
alive between frames where an unmemoized row's `Element` is consumed by
`build_node` and dropped. That is the entire cost, and it prices out at:

| | live KiB (500 rows) | per row |
|---|---:|---:|
| flat, no scopes | 4509.6 | 9236 B |
| memoized, rows read shared state | 5718.4 | 11711 B |
| memoized, each row owns a signal | 6019.3 | 12327 B |
| **→ memoizing costs** | **1208.7** | **2475 B** |
| → a scope-local signal adds, on top | 300.9 | 616 B |

`Element` is 1072 B and a row here is two of them (container + label), so **2144
of the 2475 B is the retained `Element`s themselves**; the remaining ~330 B is
the string, the children `Vec`, the `ReadSet`, and the `SpanRec`/cache entries.
Useful rule of thumb: **~1 KB per `Element` kept alive.**

**It is per *materialized* row, not per item** — which is what makes it a
non-issue for the widgets ADOPT targets:

| | cost |
|---|---:|
| `VirtualList` window (1080p, 24 px rows ⇒ ~50 materialized) | **~121 KiB**, whatever the item count |
| a non-virtualized 500-row list | ~1.2 MiB |
| a non-virtualized 5000-row list | ~12 MiB |

So a 1M-row virtual list costs the same as a 50-row one. A *non*-virtualized
list of thousands of rows is where this would bite — and such a list has larger
problems already.

### Scrolling does not accumulate

The interesting risk was churn: scrolling abandons row keys and creates new
ones, so a cache without a working GC would grow with rows-scrolled-past.
Scrolling a 14-row window through **20 000 rows**:

| rows scrolled | flat window (control) | memoized window |
|---:|---:|---:|
| 4 000 | 13144.6 KiB | 13120.3 KiB |
| 8 000 | 13278.4 | 13200.2 |
| 12 000 | 12966.9 | 12834.8 |
| 16 000 | 13381.7 | 13195.7 |
| 20 000 | 13119.1 | 12879.1 |

**Flat and indistinguishable, and identical to the unmemoized control.**
`sweep_dead_scopes` runs every build (F5 GC) and reclaims abandoned scopes; the
~13 MiB is the app's ordinary working set — glyph and shaping caches — not
memoization, which the control proves by paying it too.

*Method note:* a first pass sampled every 1000 steps over 5000 and read
4.5 → 9.4 → 8.6 → 13.1 → 6.9 MiB, which looked like unbounded oscillation. It
was the working set warming toward its steady ~13 MiB, sampled at different
points in the allocation cycle. **Adding the unmemoized control is what turned a
scary-looking trend into a null result** — the reading was real, the attribution
was wrong.

## Addendum 2 — ADOPT was implemented, and it mostly does not pay (2026-08-13)

The successor above predicted **1.54×** from making the list widgets memoize.
Built, measured, and that figure **does not transfer**. Recorded here rather
than in a separate document because it corrects this ruling's own successor.

### Two things had to be discovered first

**1. Memoizing a row is unsound without a caller-supplied dependency.**
`cx.scope` invalidates on the signals its closure *reads*, and the usual list
shape reads none — the caller pulls data out of a signal in the parent and
captures it:

```rust
let items = items.get(cx.runtime());               // read HERE
VirtualList::new(cx, "l", items.len(), .., move |i| row(&items[i]))
```

An empty `ReadSet` is always "current", so such a row would be memo-hit forever
and **freeze**. So `cx.scope_with_deps(id, deps, f)` was added — the same idiom
`cx.task(key, deps, f)` already uses — with deps stored *beside* the key rather
than folded into it, so a deps change re-runs the closure without shedding the
scope's own signals. `crates/lumen-widgets/tests/scope_deps.rs` asserts the
hazard is real, deliberately, so nobody later "fixes" `scope` into unsoundness.

**2. The 1.54× was measured on the wrong shape.** It came from `scoped_app(500)`
— a **non-virtualized** 500-row list where all 500 rows are materialized, so
memoizing saves 499 row builds. Every widget ADOPT targeted is *virtualized*.

### The measurement

100 000-item `VirtualList`, ~44 rows materialized, one row changing per frame:

| elements per row | plain | memoized | speedup |
|---:|---:|---:|---:|
| 1 | 1085.7 µs | 1076.5 µs | **1.01×** |
| 4 | 1973.6 | 1509.5 | 1.31× |
| 16 | 4088.9 | 3718.3 | 1.10× |
| 64 | 13266.1 | 12046.5 | 1.10× |

**Virtualization already captures what memoization would.** A virtual list
builds ~44 rows; the frame is dominated by *rasterizing* those 44 rows, not
building them. The two optimizations overlap, and virtualization got there
first.

### Method note — this nearly shipped as a 1.49× win

The first run measured **1.49×**, reproducibly, across three runs. It was an
artifact: whichever app ran *first in the process* paid for font loading and
first-touch caches. Swapping the order inverted the result exactly —

```
plain first:  plain 1553.5 µs, memo 1054.5 µs   → 1.47x
memo  first:  memo  1554.9 µs, plain 1063.8 µs  → 0.68x
```

— and the criterion pair, which warms up properly, had said ratio ≈ 1.00 all
along. **Reproducibility was not the check that caught it; swapping the order
was.** Three consistent runs of a biased harness are three consistent wrong
answers. `benches/benches/nodecost.rs::vlist_changed_frame` keeps the honest
pair.

### What shipped, and why

* **`cx.scope_with_deps` — kept.** It fixes a real unsoundness and is a
  prerequisite for *any* correct memoization of captured data. Its value does
  not depend on this null result.
* **`VirtualList::memoized` — kept, opt-in, documented with the table above.**
  It is correct and it does help an expensive row builder. Its doc leads with
  "measure before reaching for this" and states the 1.01× case, because an API
  that quietly costs 2.5 KB/row and a freeze footgun for 1% is worse than no
  API.
* **No perf-gate ratio was added.** A gate asserting a win that does not exist
  would be theatre.

### And it further weakens CP6

CP5.1's 20–34% and this ruling's 0.43–0.52 were both measured on the same
non-virtualized 500-row shape. On a *virtualized* list, `copy_span` runs over
~44 nodes rather than 500, so the retention saving shrinks with it. The
successor that was meant to give CP6 a beneficiary instead showed that the
beneficiary's own workload is smaller than the gate assumed.

## What this does not say

* **Nothing about the egui gap.** BENCH1's workload has no `cx.scope`, so
  neither retention nor adoption moves that ratio by a microsecond. Third time
  this has needed saying in a CP document; it keeps inviting the wrong
  conclusion.
* **Nothing against `cx.scope` itself.** F1 shipped a memoization mechanism that
  works and measures 0.648. The gap is that nothing calls it.
