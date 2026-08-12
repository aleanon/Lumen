# CP5.1 — a memo hit spends a third of its frame re-lowering

*2026-08-13. The measurement `docs/cp5-gate-decision.md` promised. Nothing was
shipped; the instrumentation was removed after the numbers were taken.*

## The question

`cx.scope` memoizes the *closure*, not the lowering: a span whose context hash
is unchanged still goes through `copy_span` → `copy_node`, which per node
inserts a tree node, refreshes flags, moves three side-table entries and builds
a **fresh taffy node**. The gate asked how much of a memo-hit frame that is —
because that is the ceiling on what a retained node graph (CP6, the campaign's
one-way door) could remove.

## Method

Two `AtomicU64` accumulators around the `copy_node` recursion in
`Headless::copy_span`, driven by a 500-row app with a per-row `cx.scope` and one
row dirtied per frame — the `text_list_scoped_changed_frame` shape. 40 frames
after warm-up, release build.

**Measured twice on purpose.** The first pass also bracketed the
`layout.leaf`/`layout.container` call *per node* — 499 `Instant` pairs per
frame at ~26 ns each — which inflated the frame from 538.0 to 557.3 µs and the
copy total from 181.6 to 191.8. The clean numbers below are from the pass
without the inner bracket; the taffy figure is corrected for it. This repo has
been burned before by instrumentation that measured itself
(`docs/build-node-drift.md`), so the correction is stated rather than absorbed.

## Result

| | µs | share of frame |
|---|---:|---:|
| memo-hit frame (500 rows, 1 dirty) | 538.0 | — |
| **`copy_span` — re-lowering unchanged spans** | **181.6** | **33.8%** |
| …of which taffy node construction | ~32.5 | ~6% |
| …of which tree insert + side-table moves | ~149 | ~28% |
| nodes copied per frame | 499 | |

So if a retained graph removed **all** of `copy_span`, a memo-hit frame would
cost **0.662×** what it costs today.

## What that does to the gate's number

`scoped_vs_flat` is **0.648** today, measured now — not the **0.787** the gate
recorded. That number predates OB2 (lazy semantics) and the `link_last_child`
quadratic fix; the ratio improved without anyone re-running it, which is its own
small lesson about quoting a stored figure.

Composing the two ratios:

```
today    0.648
ceiling  0.648 x 0.662  =  0.429
```

**The gate's own criterion is met.** It said: *"If it lands near 0.49, CP6 has a
real case for apps that memoize… If it lands near 0.787, the retained graph is
dead on measurement."* 0.43 is on the live side of that line — a memo-hit frame
would go from 1.5× faster than a full rebuild to **2.3×**.

> **Re-gated 2026-08-13 on the larger version — `docs/cp6-retained-tree-gate.md`.**
> The split below refined further: the *tree* is free (~0 ns/node), the 13.8% is
> index churn it forces, and taffy is 6.3%. Ruling was **STOP** — not on the
> number, which clears every bar, but because 0 of 51 examples and 0 shipped
> widgets use `cx.scope` at all.

## The finding that changes what CP6 should be

CP6 is written as *"persisting the arenas"*. The decomposition says that is the
**smaller share**: taffy node construction is ~18% of the re-lowering, and ~82%
is the tree rebuild plus moving `NodeMeta` / `node_style` / `node_computed`
between hash maps.

A retained *taffy arena* alone therefore buys roughly 6% of a memo-hit frame —
`0.648 → ~0.61`, nowhere near the ceiling. Getting the 0.43 needs the **tree and
the side tables** retained too, which is a materially larger change than the one
the gate was written about. Anyone re-gating CP6 should re-gate the bigger
version, or expect a sixth of the win.

## What this does not say

* **Nothing about the egui comparison.** BENCH1's workload has no `cx.scope`, so
  every number here applies to neither side of that ratio. Unchanged from the
  gate's original wording, and worth repeating because "2.3× faster" invites the
  wrong conclusion.
* **Nothing about ARM.** CP4 is still missing hardware, and per-node cost is the
  thing most likely to look different there.
* **This is a ceiling, not a forecast.** A real retained graph pays for
  invalidation tracking, and it cannot remove the flag/id refresh in `copy_node`
  (that is host state, not retained work). Treat 0.43 as the number the design
  must justify itself against, not the number it will hit.
