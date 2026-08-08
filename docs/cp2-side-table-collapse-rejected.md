# CP2.1 (collapse the side tables) — built, measured, reverted

*2026-08-08. Recorded because the plan asserted this was a win, it is not, and
the next person to read `copy_node` will have the same idea.*

## What was proposed

`copy_node` moves a node's retained work across a rebuild with four
`remove`+`insert` pairs — `prev_meta`/`meta`, `prev_node_style`/`node_style`,
`prev_node_computed`/`node_computed`, `prev_layout_style`/`node_layout_style` —
plus `root_map.insert`. The campaign plan (CP2.1) proposed folding the three
side tables into `NodeMeta` so the move becomes one map operation.

After CP1 landed, this was the *named residual*: the remaining per-memo-hit
bookkeeping, arrived at by elimination once CP2.2's clone-removal measured flat.

## What happened

Implemented in full: `NodeMeta` gained `css`, `computed` and `layout_style`;
the three maps and their `prev_` counterparts were deleted; all 23 reader sites
were repointed. 365 test binaries green, including the F0 coherence oracle.

It made things **slower**:

| | before CP2.1 | after | |
|---|---|---|---|
| `text_list_scoped_changed_frame` | 539.0 µs | 552.6 µs | **+2.5%** |
| `scope_scaling_600_nodes/300` | 891.1 µs | 937.7 µs | **+5.2%** |

(The first cut was worse still — +3.2% / +5.7% — because it reintroduced the
`LayoutStyle` clone CP2.2 had just removed. Fixing the ordering so the taffy
node is built from a borrow recovered part of it; the numbers above are the
*fixed* version, and it is still a loss.)

## Why

`NodeMeta` is stored in a `HashMap<NodeIndex, NodeMeta>` and moved per node.
The three folded fields add roughly 256 bytes (`LayoutStyle`) plus a `Style`
plus a `HashMap` header to *every* `NodeMeta`, so every insert, every remove,
and every rehash moves a substantially larger struct.

Six hashmap lookups per copied node are cheaper than making the moved payload
bigger. Lookups on small keys with FxHash (CP3.2) are a few nanoseconds;
memmoving several hundred extra bytes per node, 500 times a frame, is not.

## Decision

**Reverted. CP2.1 is retired, not deferred.**

With CP2.2's clone-removal also measuring flat and its `Rc<LayoutStyle>`
migration retired on the same evidence, **the whole of CP2 is now closed** —
and the memo-hit path's cost turns out not to be where three separate documents
said it was.

## What that leaves

CP1 already delivered the result CP2 was supposed to help with: scoped/flat went
1.442 → 0.791, so the memoized path is now *faster* than the full rebuild. M-C's
exit criterion is met without CP2 at all.

`scope_scaling` 300/50 remains at ~1.44, i.e. finer granularity still costs
something. Whatever that residual is, it is **not** the side-table churn and
**not** the `LayoutStyle` copy — both are now measured and excluded. The
remaining candidates are the per-node `tree.insert_child` + taffy node mint
(CP5 already priced the taffy half at 4.46% and declined it), and the
`>500-node superlinear inflection` found by BENCH1, which is unexplained and
tracked separately.

## The pattern worth keeping

Three plan items (CP2.1, CP2.2's Rc migration, CP6.1) were each justified by
inspection — "this looks expensive" — and each measured at or below noise. The
one change that mattered, CP1, was a complexity fix nobody had costed at all.
Inspection ranks by *conspicuousness*; only measurement ranks by cost.
