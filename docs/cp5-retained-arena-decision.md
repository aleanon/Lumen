# CP5 — the retained-arena gate: **STOP on CP6.1**

*Decision, 2026-08-08. `docs/plan-incremental-path.md:241-259` defines CP5 as
"a gate, not work", with **stop as an explicitly permitted outcome**. This is
that decision, written down as the plan requires.*

## The rule being applied

CP2.3, verbatim: *"Get the number first: if it is <5% it is not worth the
retention machinery."*

That threshold exists because the N-series died of the opposite behaviour —
committing to an XL retained-graph phase before the cheap measurement was taken.
The point of writing the number down is that the gate can say no.

## The number

`benches/benches/nodecost.rs::taffy_node_cost`, 500 nodes, matched profiles:

| | time |
|---|---|
| `mint` — build a fresh taffy tree, then compute (**what happens today**) | 105.48 µs |
| `restyle` — keep the tree, push styles via `set_style`, then compute (**what a retained arena would do**) | 75.14 µs |
| **Saving from retention** | **30.34 µs** |

Against a post-CP3.2 500-node changed frame of **680.7 µs**:

> **4.46%** — below the 5% threshold.

Both arms include `compute`, deliberately. Excluding it would flatter the
retained path by hiding the fact that layout must be recomputed either way; and
including it means the retained arm's *warm taffy cache* is credited, which is
the main thing retention is supposed to buy.

## Decision

**CP6.1 (persist `LayoutTree`) — STOP. Not scheduled.**

The saving is real but below the bar the plan set before it knew the answer, and
the machinery is not free: retained taffy nodes must be freed when their Lumen
node disappears (taffy never frees an unreferenced node), which needs
`LayoutTree::retain` plus a node-count assertion, and it introduces a whole
class of stale-layout bug where a reused node keeps a style that should have
changed. Paying that for 4.5% is the trade the threshold exists to refuse.

**This also retires LAY1** as written in the campaign plan. LAY1 and CP6.1 were
the same change described twice — an inconsistency in the plan, caught by
running the gate rather than by reading it.

## What is NOT decided here

**CP6.2 (persist `Tree`) is a separate question and remains open.**
> **Answered 2026-08-13 — `docs/cp6-retained-tree-gate.md`: STOP, and not on the
> number.** Building the tree measures at **~0 ns/node**; it matters only as the
> enabler for keeping side-table indices stable, which is 13.8% of a memo-hit
> frame (taffy a further 6.3%). Full retention would take `scoped_vs_flat` to
> 0.43–0.52, clearing every bar the campaign set — but **0 of 51 example crates
> and 0 shipped widgets use `cx.scope`**, so it speeds up a path with no users.
> Successor is ADOPT (make the list widgets memoize), not retention. CP2.3
measured the *taffy* mint cost only. The other half of the retained-arena
proposal is the per-copied-node bookkeeping in `copy_node`: 8 hashmap
remove+insert pairs, a `root_map.insert`, and a `LayoutStyle::clone()` per memo
hit, none of which this bench touches. That cost is what makes a memo hit
expensive, and it needs its own measurement before its own gate.

Sequencing note: CP1 (the O(scopes² × span) `copy_span` scan) and CP2.1/CP2.2
(collapse the four side tables, `Rc` the `LayoutStyle`) attack that bookkeeping
*without* retaining anything. They should land first — if they close the gap,
CP6.2 never needs asking.

## Caveats on this number

- **One shape.** A flat 500-node list. Deep trees, or trees with heavy
  style churn, could shift the ratio. The threshold was written against this
  shape, so applying it here is consistent, but a different workload could
  warrant re-running the gate.
- **No ARM number.** CP4 is hardware-blocked
  (`docs/cp4-arm-measurement-blocked.md`), and `plan-incremental-path.md:217-237`
  says ARM is required input to this decision. **This gate is therefore being
  decided without one of its two stated inputs** — recorded explicitly rather
  than passed over. Mobile multipliers could push the share above 5%; if an ARM
  device appears, re-run `taffy_node_cost` before treating CP6.1 as closed.
- Measured on x86_64, `lto = "thin"`, `codegen-units = 1`.
