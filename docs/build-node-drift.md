# The build_node drift is not bytes, not strings, and not the paint sort

> **RESOLVED 2026-08-09 — it was `Tree::link_last_child`, a quadratic.**
> Appending a child walked the whole sibling chain, so a k-child container cost
> O(k²). 23% of cycles in a profile; invisible to every hypothesis below. Fixed
> with a `last_child` tail pointer: 6000 rows 22539 µs → 6469 µs, and per-node
> cost went from climbing 1328→3757 ns to flat ~860–1080 ns. The falsifications
> below all stand — they were just all wrong about where it was, which is why
> this file ends by saying the term "behaves like an algorithmic term in the
> number of siblings". It was exactly that.


*2026-08-08. Four falsifications and no culprit — recorded because each one was
directing planned work.*

Localising the residual drift (commit 04d1077) established that **78% of a
3000-row changed frame is `build_node`**, and that it scales with the individual
tree's size rather than with process footprint (three 1000-row apps lower 2.17×
faster than one 3000-row app at equal total nodes). This was filed as *blocked:
needs a profiler* — `perf_event_paranoid` is 4 here and there is no valgrind.

**A profiler was the wrong instrument.** It shows *where* time goes; the question
is *why per-node cost changes with N*, and a profile of a 500-row frame and a
3000-row frame would look much the same. What that question needs is a
controlled comparison, which needs no privileges at all.

## The shape of the problem

`benches/buildscale.rs`, min-of-9 per point:

| nodes | frame (µs) | ns/node |
|---:|---:|---:|
| 500 | 664 | 1328 |
| 1000 | 1381 | 1381 |
| 2000 | 3558 | 1779 |
| 3000 | 6751 | 2250 |
| 4000 | 11369 | 2842 |
| 6000 | 22539 | 3757 |

Per-node cost grows **2.83×** between 500 and 6000 rows; total time scales as
roughly **N^1.42**. That exponent is the thing to explain.

## What it is not

**1. Not cache residency / per-node bytes.** The hypothesis was that a single
build's working set outgrows L2 (2 MB per P-core here; 3000 × 1072 B = 3.2 MB of
`Element` alone). That predicts something sharp: cost is a function of *bytes
touched*, so doubling `Element` must halve the node count at which cost/node
lifts off. Padding `Element` from 1072 to 2144 B moved ns/node by **2–8%**, and
the curves do not collapse when plotted against bytes — padded-at-1000 is 1420,
unpadded-at-2000 is 1779. Falsified.

This matters beyond the drift: cache residency was **the only surviving argument
for EL**. The RSS argument died with the 200× GPU-context finding (1.22 MB of
Tree+Element against ~270 MB process RSS). With the cache argument measured and
gone, EL has no live justification. It should be retired, not deferred.

**2. Not text-cache pressure.** N distinct row strings versus one shared string,
with node count, tree shape and element count held identical: **~2%** apart at
every N. The 2026-08 shape-cache thrash fix was real and large, but it is not
what remains.

**3. Not the paint-order sort** — which I had introduced earlier in this same
campaign, and which was the best structural candidate on the list. `z-index`
sorting made a container's paint cost O(k log k) in child count where document
order was O(k), and it ran for every node on every frame even though nearly
nothing sets `z`. A fast path (walk the sibling chain directly when no child has
a non-zero `z`) measured **flat** — 3871 → 3843 ns/node at 6000, inside noise.

The fast path is kept, on complexity grounds rather than measured time: it is
strictly less work for the universal case and removes a per-node allocation.
**It is not a performance win and should not be cited as one.** That makes five
items in this campaign justified by inspection and measured at or below noise.

**4. Not allocation** — ruled out earlier by `nodecost.rs`'s counting allocator.

## What is left

The superlinear term is unattributed. What the falsifications do is redirect the
search: it is **not** in data layout, so the SoA/field-packing family of fixes
(N1, EL) is aimed at the wrong thing. It behaves like an algorithmic term in the
number of siblings or nodes — the same place CP1's win came from, where one
nested-span scan was O(scopes² × span) and no cost model had it.

Candidates not yet separated, in the order they are worth testing: taffy's
flexbox pass over a very wide container; the semantics/`dep_index` build (3% of
a 3000-row frame, so it cannot be the whole term but could scale worse than
linearly); and the display-list emission itself.

`benches/buildscale.rs` is the instrument for whichever is next — it takes a
shape parameter, so "same node count, different tree shape" is a one-line
change. It is deliberately **not** wired into `perf_gate.sh`: it prints a table
for comparison and there is no ratio in it CI could hold to.

## The method note

The first version of this harness reported 0.1 µs per frame at every N and a
tidy declining ns/node curve. It was measuring a pump that never rebuilt,
because nothing had dirtied the root signal. A "measurement" that produces a
plausible-looking table from a no-op is the failure mode these experiments are
most exposed to — which is why every number above is stated with the shape that
produced it, and why the padded-`Element` run exists at all: a *prediction that
could fail* is worth more than a profile that cannot.
