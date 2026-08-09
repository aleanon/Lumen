# The patch path is 3.6× cheaper — and almost nothing takes it

*2026-08-09. Follow-up to `docs/six-x-gap-investigation.md`, prompted by the
question "is the gap a cost of the signals?"*

## No — signals are not the cost

| | measured |
|---|---|
| view closure (all signal reads) | **65 µs of a 1493 µs frame — 4%** |
| `Signal::update` | **16 ns** |
| re-addressing 1000 per-row signals (ADR-021) | **18.2 µs**, 0 allocations |

The frame is dominated by per-node *lowering* (887 µs, 59%), which is
content-independent and runs whether or not any signal changed shape.

## But reactive granularity decides which path a frame takes, and that is worth 3.6×

Lumen has two paths. Measured at 1000 rows, same tree, same signal flipping:

| path | µs/frame |
|---|---:|
| rebuild (signal read at the root) | **2322.5** |
| **patch** (signal read only inside a bound background) | **653.8** |

**3.6×.** A patch frame runs no view closure, builds no nodes, and touches no
taffy — it re-evaluates the stale bindings, writes `meta.background`, and
repaints. For scale: egui's whole frame at this size is 267 µs, so a patch frame
is 2.4× off it rather than 6×.

## Why almost nothing takes it

`patch_bg_bindings` is the entire patch implementation, and it handles **one
property: background colour**. The other reactive bindings — `dyn_text`,
`dyn_classes` — record their dependency keys but are evaluated *inside*
`build_node`, so a change to either re-enters the full rebuild.

The boundary is not arbitrary. Background is **paint-only**: patching it cannot
change any box, so layout can be skipped. Text and classes can change size, and
Lumen has no incremental layout to fall back on — `LayoutTree::set_style` exists
with **zero production call sites**, and a fresh `TaffyTree` is minted every
rebuild, discarding taffy's per-node cache.

So the patch path is narrow because *incremental layout is missing*, not because
the reactive system is coarse.

## What this says about `#[derive(Reactive)]` per-field

It would not move any number above **on its own**. A per-field derive changes
how signals are *declared*, not what a write triggers, and coarser fields
invalidate more, so more frames would take the expensive path rather than fewer.

The lever the measurement points at is different: **widen the set of properties
that can be patched**, which is a property of the renderer and layout, not of
the reactivity primitive. In order of cost:

1. **Paint-only props** (opacity, border colour, shadow, text colour) — the same
   shape as background, each a small addition to the existing patch path.
2. **Size-affecting props** (text, classes) — needs `set_style` + `mark_dirty`
   wired to a persistent `TaffyTree`, i.e. the incremental-layout work that was
   parked on the false premise that taffy could not be partially re-solved.

A per-field derive is still worth having for *ergonomics* — it removes the
`Dynamic::new(move |rt| …)` plumbing that currently makes the fast path
laborious to opt into, and the fast path being laborious is itself part of why
nothing uses it. But it should be understood as an API convenience that makes an
existing 3.6× reachable, not as a performance change.

## Addendum: would a derive remove the dependency graph?

Asked directly, so answered with the same discipline.

**There is no per-frame dependency graph left to remove.** OB3 deleted the eager
reverse index (`rebuild_dep_index`), which used to clone a `String` per
dependency per node every rebuild; `what_depends_on` is now computed on demand
for the agent RPC that is its only reader. What remains in the hot path is a
read *set*: `collect_reads` pushes one `Vec` frame, each `Signal::get` pushes its
id, and the frame ends with one snapshot.

**Cost: ~1%.** The perf gate measures re-addressing 1000 per-row signals —
including recording their reads — at **18.2 µs with zero allocations**, against
a 1493–2322 µs frame. A derive that eliminated dependency tracking *entirely*
would buy about one percent.

Three things a compile-time derive cannot do, which matter more than the 1%:

* **It cannot supply the instance mapping.** Static analysis can prove "this
  widget depends on `Item::name`"; it cannot say *which of 1000 rows*. A runtime
  map from (field, index) → node is still required — which is exactly what
  ADR-021's typed keys are, at the 18.2 µs above.
* **It over-approximates dynamic reads.** Runtime tracking records that
  `if flag { a.get() } else { b.get() }` depended on `a` *or* `b`, not both. A
  static derive must assume both, so more writes are treated as invalidating,
  and more frames take the 2322 µs path instead of the 654 µs one. That is the
  opposite of the direction the 3.6× points.
* **It does not remove the work the dependency implies.** Knowing precisely that
  a text binding changed still leaves a re-measure and a relayout to perform,
  and there is no incremental layout to perform them with. Discovery was never
  the blocker.

**Where static knowledge would genuinely pay:** classifying a binding's *target
property* at compile time. Patch eligibility is currently decided by which
builder the author called (`bind_background` = paint-only, therefore patchable).
A derive that knew a binding writes only a paint-only property could make that
routing total and checkable, rather than one hand-wired case — turning the
patch path from a special case into the default for every prop that qualifies.
That is an argument for the derive, but it is an argument about *dispatch*, not
about dependency tracking.
