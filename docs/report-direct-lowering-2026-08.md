# Report: direct lowering, seven prototypes in

**Branch** `exp/widget-trait` · **2026-08-25/26** · **1108 workspace tests, clippy clean**

Seven prototypes across two rounds tested whether `Element` — the uniform,
1072-byte record a widget produces and `build_node` immediately reads back
apart — can be removed, with widgets writing straight into the retained SoA
`Tree` and its per-node side table.

**No blocker was found.** Every load-bearing behaviour survived, two of them
only after a design was wrong first. What follows is the evidence, the four
bugs found along the way, and the honest ledger.

---

## The idea

Today:

```
widget → Element (1072 B, uniform) → build_node reads 41 fields → Tree + NodeMeta
                                                                   ↑ Element dropped
```

Direct:

```
widget → TreeSink → Tree + NodeMeta
```

`Element` is pure marshalling. Every field it holds is copied into the SoA tree,
taffy, or the side table, and then it is dropped.

**The agent never read it.** `lumen-agent` has zero references to `Element`; it
reads `SemanticsNode`, derived from the side table. Observability was never at
stake.

---

## Round 1 (prototypes 1–3)

| | finding |
|---|---|
| **Lowering** | allocations **−23%**, time **−10.8%**, peak bytes only −10% |
| **Cascade** | composes; `apply_css_to_element` was already a pure function onto a target |
| **Ordering** | a real hazard — silently unstyled nodes — made unrepresentable by type states |
| **Memoization** | survives; the fast path never touched `Element`. **415 µs → 15–29 µs** |

The peak-memory case is **weaker than first claimed**: building the staging tree
peaks at 2.63 MB but the destination peaks at 5.07 MB, so `Element` is the
smaller half and removing it moves peak ~10%, not the 6.4× originally projected.
The real arguments are allocation churn and architecture, not footprint.

---

## Round 2 — the four remaining unknowns

Each was something `build_node` does against a **mutable `Element`**. Ordered by
risk of being a blocker, not by size.

### P1 — text measurement feeding layout · **works**

`build_node` shapes a text leaf and writes a fixed size onto the style before
taffy, reconciling three inputs that arrive at different times: the widget's
width, the cascade's `text-wrap`, and the content. In the sink they meet at
`end()` — the moment all three are known. The reconciliation point exists
without an element; it moved from "the element everyone mutates" to "the call
that closes the node".

Six properties hold, including the two `build_node` documents as hard-won:

* an author-fixed axis is **never** overwritten by a measurement;
* a percentage width **cannot** feed the wrap width, because the containing
  block is not resolved until layout runs, which is after measurement;
* `text-wrap: nowrap` works — the load-bearing case, since it proves the
  **cascade reaches measurement through composition**;
* and both paths measure the same box, to the pixel.

### P2 — overlay routing and the memo context · **found a real bug**

Expected to bite, and did. The engine guards every splice with `span_ctx_hash`:
ancestor chain, container size, overlay flag, hidden/disabled depth. All of it
feeds the cascade, so a span may only be reused when the whole *outside context*
is unchanged. **The prototype's `scope()` checked only the caller's `dep`.**

Demonstrated before fixing, as the plan required. With the guard removed:

* a button retained under `.calm` and spliced under `.danger` keeps the styling
  it got under `.calm`;
* a button retained outside an overlay keeps `z = 0` after moving into one —
  **painting under the page it is meant to float above**.

Both tests fail without the guard and pass with it. `SpanRec` now carries the
context it was built in, hashed with `IdHasher` (a collision splices a stale
subtree — a wrong view, not a slow frame).

**Cost:** memo frames went ~11–22 µs to ~15–29 µs against a ~450 µs full
rebuild. Not free — it hashes the ancestor chain per scope — but memo is still
15–30×.

### P3 — transitions · **found a deadlock; the engine shows the way out**

The hardest finding, and the most transferable.

Blending itself composes like the cascade: a function from `(id, clock)` onto the
resolved `Style`, never needing an element. The coupling to memoization is where
the work was. `splice_span` refuses any span containing an animating node,
because its styles are mid-interpolation.

**The first design deadlocked on itself.** It marked nodes `animating` during
`resolve` and refused spans containing a marked node — but a node is only marked
while being resolved, is only resolved if its span was *not* spliced, and the
span is only refused if the node is marked. The first memoized frame spliced the
animating node, it never resolved again, and the transition froze at frame zero:
*the exact failure the check exists to prevent, caused by the check.* The test
reported `[0.0, 0.0, 0.0, 0.0, 0.0]`.

The engine's `span_has_running_anim` tests the **retained meta's id against an
animation registry held in engine state**, populated by whatever started the
transition — never by the build. The registry is knowable before the span is
examined, which is what breaks the cycle.

> **Constraint for any direct-lowering design:** animation state must be keyed
> independently of the build. Derive it from the build and it cannot bootstrap.

Five properties hold, four of which fail if the refusal is removed: monotonic
blending across frames, no freeze under memoization, **only** the animating span
refused (one hover must not cost a full rebuild), splicing resumes once the blend
completes (or an app that ever animated stays expensive forever), and a frame
with no animation never pays for the span scan.

### P4 — damage tracking · **not affected**

`damage_between(prev, next)` is a prefix/suffix diff over two **display lists**,
downstream of the tree. Nothing in it asks how the tree was built.

The risk is the proviso, and it is sharper than it sounds: splicing reuses nodes
**without touching them** — no `resolve`, no `end`, no writes. Any observable
state established only during a rebuild would silently vanish from a spliced
frame. And because damage is a *prefix* scan, a reordering would not mislocate
the rectangle but defeat the scan entirely and report the whole frame changed.

So the tests target that invariant rather than damage itself, comparing **every
field a painter reads** — role, id, label, value, classes, background, corner
radius, measured width/height, z, content presence, child count — between an
incrementally spliced sink and a fresh one, across churn mixing memo hits, dirty
scopes, an overlay transition and a running animation. Four properties, all
passing, including that measured text boxes survive five splices (measurement
happens in `end()`, which a spliced node never reaches).

---

## Final measurements

One arm per process, 9 repeats, median. Deterministic metrics are trustworthy;
timings carry 3–28% spread and are directional.

| lowering, 2501 nodes | median | vs Element |
|---|---:|---:|
| `element` | 3058 µs | — |
| `direct` | 2729 µs | **−10.8%** |
| `element_styled` | 3693 µs | — |
| `styled` (direct) | 3290 µs | **−10.9%** |

| allocation, 2501 nodes | via `Element` | direct |
|---|---:|---:|
| allocations | 8 707 | **6 706** (−23.0%) |
| total bytes | 12.92 MB | 10.34 MB (−20.0%) |
| peak live bytes | 8.73 MB | 7.86 MB (−10.0%) |

| memoized frame, 500 scopes, 1 dirty | median | composition |
|---|---:|---|
| full rebuild | ~450 µs | 500 rebuilt, 1501 freed |
| memoized | **15–29 µs** | 499 spliced, 1 rebuilt, 1497 reused, 4 freed |

---

## Bugs the prototypes found

Four, three of them in designs that looked right:

1. **Cascade ordering** (round 1) — a widget resolving before declaring its
   classes is silently unstyled. `ProgressBar` shipped with it. Now
   unrepresentable: three mistakes fail to compile, pinned by `compile_fail`
   doctests.
2. **Unbalanced ancestor stack** — with no stylesheet loaded, `resolve` pushed an
   ancestor but stored no style, so `end` never popped; 601 leaked entries.
   Caught by `assert_balanced()`, added one prototype earlier for a different
   reason.
3. **Missing context guard** (P2) — spliced spans reused across changed
   surroundings.
4. **Animation deadlock** (P3) — the refusal that prevented freezing caused it.

Plus two **harness** flaws that inverted results before being caught: boxed
child closures (showed −10.3% where the fixed harness showed −33%), and
per-property `meta.get_mut` hashing (made direct *slower* than the path it was
meant to beat).

---

## Method, and three numbers that were wrong

This workload allocates ~10 MB per frame, so **criterion timed allocator residue
rather than code**: the same `lower_direct` measured **941 µs and 2.71 ms** in
two groups of one binary. Timing moved to one-arm-per-process binaries with
their own warmup and median-of-many, repeated 9×.

Three figures reported earlier on this branch were artifacts. The rules adopted
after, and kept for the whole of round 2:

* deterministic metrics first — frame composition, node counts, allocation
  counts; timings directional only;
* **demonstrate the bug before fixing it**, or a passing test proves nothing —
  this is what made P2 and P3 real rather than assumed;
* `assert_balanced()` after every frame.

---

## The ledger

**For**
* ~23% fewer allocations, ~11% faster lowering, cascade cheaper (238 vs
  366 ns/node)
* widgets carry only their own data (`Element` was uniform at 1072 B)
* memoization intact and genuinely O(changed)
* observability untouched — the agent never read `Element`
* the ordering hazard is unrepresentable, not merely documented

**Against**
* `AppSnapshot` and the golden tests that read `Element` fields need rework
* the engine conversion is large — `build_node` is ~500 lines
* peak memory barely moves; the footprint argument does not hold
* a real new obligation: animation state must be keyed independently of the build

**Still unprototyped**
* `@keyframes` timelines (only property transitions were modelled)
* container queries — `MediaContext.container` feeds the context hash and comes
  from the *previous* layout
* hot reload and `AppSnapshot` restore
* the actual engine conversion: `build_node`'s interaction with hidden subtrees,
  error boundaries, and the F3.5 in-place patch path

**Assessment.** The design is sound and modestly cheaper. It is not a
performance win worth the conversion on its own; it is worth doing if the goal
is the architecture — a widget that carries only what it needs, a trait third
parties can implement, and no uniform record in the middle. That was the stated
goal, and nothing found across seven prototypes rules it out.

---

## Artifacts

**Prototype** `crates/lumen-widgets/src/direct.rs` — `TreeSink`, `Direct`,
typestate guards, cascade, memoization, text measurement, overlay, transitions.

**Tests** (29) `direct_cascade.rs` 10 · `direct_memo.rs` 5 · `direct_text.rs` 6 ·
`direct_overlay.rs` 4 · `direct_anim.rs` 5 · `direct_damage.rs` 4 ·
`third_party_widget.rs` 4 · `composition_showcase.rs` 1

**Instruments** `benches/benches/lowercost.rs` · `benches/src/bin/lowerprobe.rs` ·
`lowertime.rs` · `memotime.rs`

**Prior docs** `experiment-widget-trait-2026-08.md` ·
`prototype-direct-lowering-2026-08.md` · `plan-direct-lowering-unknowns.md`
