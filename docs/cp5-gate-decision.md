# CP5 — the written gate, re-run with `scoped_vs_flat = 0.787`

> **CP5.1 ran, 2026-08-13 — the gate's bar is met.** `docs/cp5.1-memo-hit-lowering.md`.
> Re-lowering unchanged spans is **33.8%** of a memo-hit frame, so removing it
> all would take `scoped_vs_flat` from **0.648** (today, re-measured — not the
> 0.787 recorded below, which predates OB2 and the quadratic fix) to **~0.43**.
> That is the live side of the "near 0.49" line this document drew. **But the
> decomposition changes what CP6 has to be:** taffy node construction is only
> ~18% of the re-lowering; the rest is the tree rebuild and the side-table
> moves. "Persisting the arenas" as specified below buys ~6% of a frame, not the
> ceiling. Re-gate the bigger version — retained tree *and* side tables — or
> expect a sixth of the win. Still says nothing about the egui gap, and CP4
> remains missing.

*2026-08-09. The gate asks four questions and permits "stop". Answers below,
with numbers.*

## Input status

| input | state |
|---|---|
| CP1–CP3 desktop suite | **available** — this document |
| post-CP1/CP2 ratio | **0.787** (`scoped_vs_flat`, gate ceiling 0.90) |
| CP2.3 taffy-mint cost | **4.46%**, against CP2.3's own 5% bar |
| per-node cost breakdown | **available** — `docs/six-x-gap-investigation.md` |
| **CP4 ARM baseline** | **MISSING — no ARM hardware.** See below for what it can and cannot change |

The gate specifies desktop **and** ARM. Half the input does not exist, so this
is a **desktop-only ruling**, and it is written to be robust to the gap: the
outcome below is *stop on the one-way door*, and an ARM number can only make
per-node cost look worse, never better. It could therefore overturn a "stop"
later — which is why the successor task is a measurement, not a decision.

## Q1. Is the scoped path now cheaper than the flat path? By how much?

**Yes — 0.787, from 1.442 before CP1.** The memoized path is 21% cheaper than
the full rebuild it replaces. M-C's exit criterion is met.

It is also the number that reframes everything else: **21%, not 99%.** A memo
hit with one dirty row in a thousand still costs four fifths of rebuilding
everything, because `copy_span` re-lowers the copied subtree — rebuilding taffy
nodes and re-inserting side-table entries per node. The incremental architecture
is incremental in *closure evaluation* only.

## Q2. What is the residual per-node cost, and what is it made of?

**~875 ns per node in `build_node`, and it is content-independent** — 1000 empty
containers cost ~740 ns each with no text at all. At 1000 rows the frame is:

| phase | µs | share |
|---|---:|---:|
| view closure | 65 | 4% |
| `build_node` | 887 | 59% |
| layout (taffy) | 136 | 9% |
| paint | 319 | 21% |
| semantics | 86 | 6% |

Inside `build_node`: text 15%, taffy leaf 10%, `NodeMeta` 8%, `.lss` 1%, tree
allocation ~0, and **~50% diffuse with no single owner**. Eight candidate causes
were falsified by measurement (`docs/six-x-gap-investigation.md`). There is no
hotspot; the per-node path is simply long.

## Q3. Does anything from the retired N-series now justify itself?

**The retained node graph (N3): its retirement record is void, but the measured
headroom does not justify the one-way door.** Both halves need saying.

*The record is void.* N3 was retired on two grounds and neither survives:

* **"Marginal — 1.6 pp of a 60 Hz frame at 500 nodes."** This figure is on the
  campaign's own **quarantine list**: *"Retained graph = 1.6 pp — no derivation
  exists."* It was never measured.
* **"Blocked on decoupling semantics ids from arena slots"** — `SemanticsNode`
  was constructed from `node.index()`, serialized as the public `node-<index>`
  handle and pinned in `conformance.rs`. **Phase 1 removed this.** Semantics now
  take a `NodeHandle` (`nx-<hex>`) as public identity, and the conformance test
  asserts a round-trip property rather than a literal.

So the item was retired on a number that did not exist and a blocker that no
longer does. That is worth correcting in the record regardless of the decision.

*The headroom is bounded.* From the measured phases, if a memo hit skipped
lowering entirely for unchanged subtrees:

```
today   scoped frame = 0.787 x flat
ceiling                 closure 65 + layout 136 + paint 319 + semantics 86
                      + ~240 unmeasured overhead   =  ~846 µs of 1732
                      = ~0.49 x flat
```

**~1.6× on the memoized path, and exactly nothing on the flat path.** That
second clause is the one that decides it: BENCH1's workload has no `cx.scope`,
so a retained-lowering optimization would not move the egui comparison by a
microsecond. The 6× gap and this optimization are disjoint.

*Other N-series items:* SoA side tables beyond CP2.1 — no (CP2.1 itself measured
−2.5%/−5.2% and was reverted). Subtree texture caching — no, unchanged: it is
conditional on N3 and makes the display list backend-dependent.

## Q4. Is R4 / the multi-`TaffyTree` split still correctly parked?

**Yes.** Layout is 136 µs of a 1493 µs frame — 9%, against the 10% that parked
it originally. Desktop agrees with the original ruling. ARM may not, and that is
one of the things CP4 is for.

---

## Decision: **stop on CP6 as specified; open a measurement, not a build**

CP6 is the campaign's **hard one-way door** (persisting the arenas). The measured
case for walking through it is ~1.6× on the memoized path only, with no effect
on the competitive gap. That is not enough for an irreversible XL phase, and
committing to one on an estimate is precisely the failure the N-series is
retired for — *"the N-series' failure was committing to an XL phase before the
cheap measurements were taken."*

**But the estimate is now the cheap measurement's job, not the gate's.** The
successor task is bounded and reversible:

> **CP5.1 — prototype "memo hit skips lowering" and measure the real ratio.**
> Keep the lowered node for an unchanged span instead of re-deriving it in
> `copy_span`. Report the new `scoped_vs_flat`. Ship nothing.

If it lands near 0.49, CP6 has a real case for apps that memoize, and it should
be re-gated *with* CP4 by then. If it lands near 0.787 — because the side-table
and taffy bookkeeping dominate rather than the lowering itself — the retained
graph is dead on measurement rather than on a quarantined number, which is a
better grave than the one it has.

## What this does not do

**It does not close the 6× gap to egui, and no item on the N-series list does.**
That gap is ~5 per-node structures against egui's one, paid on every frame on
both paths. Closing it means building fewer structures per node, not building
them less often — a different question from the one CP5 was written to answer,
and one that currently has no candidate with more than 8% behind it.
