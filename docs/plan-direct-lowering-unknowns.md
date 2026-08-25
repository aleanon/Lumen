# Plan: the remaining unknowns in direct lowering

Three prototypes found no blocker (`prototype-direct-lowering-2026-08.md`). Four
things `build_node` does against a **mutable `Element`** were deliberately
excluded, and each could still be one. This plan prototypes them, smallest
slice first, measuring at every step.

Ordered by *risk of being a blocker*, not by size — a blocker found on day one
is worth more than three easy wins.

## P1 — text measurement feeding layout

**Why it might not work.** `build_node` shapes text and writes a fixed size onto
`style` *before* taffy sees it, using the wrap width derived from `style.width`
and the `text-wrap` property from the cascade. Three inputs that arrive at
different times, currently reconciled by mutating one element.

**Slice.** Give `TreeSink` a `TextEngine`; measure at `end()`, where the widget's
style, the cascade's overrides and the content are all known.

**Pass.** Direct and Element paths agree on the laid-out size of wrapped and
unwrapped text, `text-wrap: nowrap`, and an explicit-width paragraph.

**If it fails.** Measurement may need the *containing block* (a percentage
width), which is not known until layout runs. Fallback: a measure callback
handed to taffy, which is how taffy expects leaf measurement anyway.

## P2 — overlay/z routing, and the memo context it feeds

**Why it might not work.** This is the one I expect to bite. `span_ctx_hash`
hashes the ancestor chain, container size, overlay flag, hidden and disabled
depth — the "outside context" a retained span must match before it may be
spliced. **The prototype's `scope()` checks only the caller's `dep`.** A span
whose surroundings changed would be reused wrongly: same data, different
cascade match.

**Slice.** Overlay depth + `OVERLAY_Z` on the sink; a `ctx_hash` over the
ancestor chain and overlay/disabled state; `scope()` refuses a splice when it
differs.

**Pass.** A scope moved into an overlay, or under a different ancestor, is
rebuilt rather than spliced — proven by a test that would produce a *wrong tree*
without the check.

**If it fails.** Fall back to hashing the full desc stack per scope (what the
engine does) and measure the cost; the engine's own note says it was 8.3% of a
memoized frame before `IdHasher`.

## P3 — transitions and `@keyframes`

**Why it might not work.** `apply_transitions(&el.id, &mut css)` mutates the
resolved style mid-flight, and `splice_span` **refuses to splice any span
containing an animating node** because its styles are mid-interpolation. So
animation and memoization are coupled: get it wrong and an animating node
freezes at the frame it was first spliced — a silent, visual-only bug.

**Slice.** Apply transition blending to the composed style at `resolve()`;
register animating nodes; make `scope()` refuse spans containing them.

**Pass.** A transitioning node keeps changing across frames while its
*siblings* still splice. The freeze must be demonstrated first (test the bug,
then fix it), or the test proves nothing.

**If it fails.** Animating nodes may need to opt out of memoization entirely at
the scope level, which is a cost worth measuring rather than a blocker.

## P4 — damage tracking

**Why it might not work.** Least likely of the four: damage is computed by
diffing display lists downstream of the tree, so it should be indifferent to how
the tree was built. But splicing reuses nodes *without touching them*, and if
damage is inferred from "which nodes were written this frame" it would report
`None` for a frame that did change.

**Slice.** Derive a damage hint from the sink's frame stats and check it against
what actually changed.

**Pass.** A frame that splices everything reports no damage; a frame with one
rebuilt scope reports damage bounded to that scope; neither under-reports.

**If it fails.** Damage stays where it is (display-list diffing), and the sink
simply does not participate — no worse than today.

## Method

Per the previous round's lessons, which cost three wrong numbers:

* **One arm per process** for timing; criterion times allocator residue on this
  workload.
* **Deterministic metrics first** — frame composition, node counts, allocation
  counts. Timings are directional only.
* **Demonstrate the bug before fixing it** where a test is meant to prove a
  hazard is real.
* **`assert_balanced()` after every frame** — it has caught two bugs already.
