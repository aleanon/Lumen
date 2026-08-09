# BENCH1 — first competitive measurement (Lumen vs egui), 2026-08-08

The project's performance bar is **relative**: A = match the industry leader,
A+ = surpass it. It had never been benchmarked against another framework. This
is the first external number it has.

Harness: `benches-competitive/` (its own workspace, excluded from the root one
so foreign toolkits never enter Lumen's dependency graph or `cargo-deny`
closure). Run with `cargo bench --bench vs_egui`.

## What was measured

One frame producing draw commands from an N-row text list, one row of which
changes per iteration:

* **Lumen** — `Headless::pump()`: re-run the view closure, reconcile, lay out
  (taffy), paint to a display list, rebuild the semantics tree.
* **egui** — `Context::run()` + `tessellate()`: re-run the UI closure, lay out,
  tessellate shapes into meshes.

Both viewports 400×800. Both compiled with `lto = "thin"`, `codegen-units = 1`
— matched to the root workspace's release profile, because without that the
harness would benchmark a differently-compiled Lumen than users get.

## Re-measured 2026-08-09 — two corrections, and the gap is 3.3×

Two things changed since the 2026-08-08 numbers below. Both were defects in what
was being compared, not improvements in what egui does.

**1. `Tree::link_last_child` was quadratic.** Appending a child walked the whole
sibling chain, so building a k-child container cost O(k²). It was 23% of cycles
in a profile and invisible to eight rounds of hand-bisection. A tail pointer
fixed it; 3000 rows went 7052 µs → 3161 µs.

**2. The harness was not comparing the same stopping point.** Its own header
claimed both sides stop at "ready to hand to a renderer", but `pump()` →
`paint()` → `render_frame` **rasterized into an `RgbaImage`** while egui's
`tessellate()` stops at meshes. Lumen was charged for a CPU rasterizer egui
never ran. The Lumen app now builds with a `NullRenderer`, so it stops at the
display list — the artifact that is egui's meshes' actual counterpart.

| rows | Lumen | egui | ratio | ratio as first published |
|-----:|------:|-----:|------:|------:|
| 100 | 123.8 µs | 38.1 µs | **3.25×** | 10.3× |
| 250 | 258.3 µs | 77.9 µs | **3.32×** | 7.0× |
| 500 | 472.1 µs | 143.3 µs | **3.29×** | 5.8× |
| 750 | 668.6 µs | 208.3 µs | **3.21×** | 5.5× |
| 1000 | 900.2 µs | 273.7 µs | **3.29×** | 6.5× |
| 1400 | 1215.4 µs | 381.9 µs | **3.18×** | 6.5× |
| 2000 | 1969.7 µs | 551.7 µs | **3.57×** | 5.9× |
| 3000 | 3058.7 µs | 830.5 µs | **3.68×** | 8.8× |

**The ratio is now flat at ~3.2–3.7× across a 30× range of sizes.** That is the
more informative result: a constant ratio means the two frameworks now scale
the same way, and what separates them is a constant factor, not an algorithmic
difference. Per row, Lumen is ~1.0 µs against egui's ~0.28 µs.

The 100-row point is the clearest evidence for correction 2: it was 10.3×, the
worst ratio in the table, and it is now 3.25×, the best. That entire anomaly was
rasterization of a 400×800 frame, which is nearly fixed-cost and therefore
dominated the smallest case.

What still separates the two is documented in the caveats below and is mostly
capability: Lumen rebuilds a **semantics tree** every frame (egui has no
equivalent), shapes text with parley/swash (full Unicode + bidi against egui's
simpler layout), and maintains a **retained tree + taffy** where egui appends to
a mesh. Those are choices, not defects, and they are what the remaining ~3×
buys.

## Result (original measurement, 2026-08-08)## Result (original measurement, 2026-08-08)

Measured at eight sizes after the cache fix below (criterion, 2 s warm-up /
4 s measurement; egui at 1 s / 2 s).

| rows | Lumen | egui | Lumen / egui |
|-----:|------:|-----:|-------------:|
| 100 | 382.7 µs | 37.1 µs | **10.3×** |
| 250 | 529.2 µs | 75.2 µs | **7.0×** |
| 500 | 805.5 µs | 138.3 µs | **5.8×** |
| 750 | 1 131.5 µs | 204.5 µs | **5.5×** |
| 1000 | 1 731.9 µs | 266.6 µs | **6.5×** |
| 1400 | 2 380.0 µs | 368.9 µs | **6.5×** |
| 2000 | 4 061.7 µs | 690.4 µs | **5.9×** |
| 3000 | 7 051.9 µs | 797.0 µs | **8.8×** |

(egui's 2000-row point was the noisiest measurement on either side, CI
632–767 µs; the 3000-row ratio inherits that noise. Don't read the 8.8×.)

## The scaling was the real finding — and it was a cache defect

| | 100 → 500 (5× rows) | 500 → 2000 (4× rows) |
|---|---|---|
| **Lumen, as first measured** | 2.07× | **9.52×** |
| **Lumen, after the fix** | 2.11× | **5.04×** |
| **egui** | 3.73× | 4.99× |

The original run showed Lumen scaling *better* than linear to 500 rows and then
sharply superlinear — 4× the rows costing 9.5× the time. That was called out as
a ceiling rather than a backlog item, and as the most valuable thing the
benchmark surfaced. It was.

**It was `lumen-text`'s shape/run cache thrashing, not a scaling property of the
architecture.** Both caches evicted by dropping an arbitrary half (`retain` over
hash order, no recency information). That is safe only while the live working
set fits in `cap / 2`; past that, the sweep drops to `cap / 2`, the same frame
re-shapes what it still needs, and the cache re-crosses the cap — locking into
permanent thrash after a *single* crossing. At 2000 rows this cost **1183
re-shapes per frame**. Full reasoning, the rejected alternatives, and the
epoch-based replacement are in `.ai_docs/07-decision-log.md` (2026-08-08).

Two things are worth carrying forward from how this looked before it was found:

* **The benchmark's own 2000-row number was measuring a pathology, not the
  framework.** A cliff between two adjacent sizes is a much better signal than a
  ratio; bisecting the size axis (100 → 3000 in eight steps) located it in one
  run, where the original three-point table could only show that *something*
  happened.
* **It was never a 2000-node problem.** The trigger (cumulative distinct strings
  crossing 2048) and the lock-in condition (live set above 1024) are different,
  so 1400 rows measured clean while sitting 31 frames away from the same cliff.
  Any app holding more than ~1024 distinct strings would have hit this
  eventually, from a single changing label.

Note this was *not* the O(scopes² × span) `copy_span` defect the CP-series
targets: this app has no `cx.scope` at all. Two independent complexity defects,
neither costed by any document, both found only by measurement.

## What remains — measured, not guessed

Lumen's marginal cost per row still grows with N — 1.06 µs/row over 100→500,
2.33 over 1000→2000, 2.99 over 2000→3000 — where egui's is flat at ~0.26. So the
*cliff* is gone and the scaling now matches egui's over 500→2000, but a gentle
superlinearity remains. Per-stage, per frame (µs, temporary instrumentation over
200 frames):

| rows | lower | layout | paint | semantics | closure | frame |
|-----:|------:|-------:|------:|----------:|--------:|------:|
| 500 | 302.0 | 70.7 | 670.3 | 34.2 | 33.0 | 1 151.7 |
| 1000 | 972.5 | 142.2 | 513.5 | 67.7 | 64.8 | 1 876.5 |
| 2000 | 2 716.8 | 292.7 | 378.7 | 138.6 | 127.5 | 3 851.4 |
| 3000 | **5 453.7** | 435.6 | 403.0 | 228.5 | 190.0 | 7 013.9 |

Four things follow, three of which contradict what one would assume:

1. **The drift is entirely in `build_node`** — the lowering pass — which is 78%
   of the 3000-row frame and grows 18× for 6× the rows (0.60 → 1.82 µs/row).
   Everything else is linear or better.
2. **The view closure is not the problem.** User code (3000 `format!` calls plus
   widget construction) scales 5.8× for 6× rows — linear.
3. **Paint *falls* as rows grow** (670 → 403 µs). It is viewport-clipped, so
   only visible rows cost anything. More rows means more rows off-screen.
4. **OB2 (lazy semantics) is not the lever here.** Semantics is 228 µs of a
   7014 µs frame — **3%**. Making it lazy is still right for other reasons, but
   it cannot close a 6× gap, and this document should not have implied it would.

The cost scales with **the size of the individual tree being lowered, not with
process-wide memory pressure**: three 1000-row apps pumped every frame have the
same total node count and live footprint as one 3000-row app, but lower 2.17×
faster (2604 µs vs 5646 µs). Pre-sizing `meta` and `built` from the previous
frame's node count gained ~2.5% and left that ratio unchanged, so repeated
reallocation and rehashing are ruled out.

### The working-set hypothesis, tested and falsified

The obvious explanation was that a single build's working set outgrows L2 —
3000 nodes × `size_of::<Element>() == 1024` is ~3 MB before `Tree` and `meta`.
That would have **re-motivated EL (shrinking `Element`)** on cache-residency
grounds, a different argument from the RSS one that parked it.

It was tested by padding `Element` to 2048 bytes behind a temporary feature and
re-measuring. If footprint drove the drift, 1500 padded rows should behave like
3000 unpadded ones. They do not:

| rows | 1 KB nodes | 2 KB nodes |
|-----:|-----------:|-----------:|
| 750 | 0.812 µs/row | 0.841 µs/row |
| 1500 | 1.166 (×1.43) | 1.096 (×1.30) |
| 3000 | 1.834 (×2.26) | 1.972 (×2.35) |

Doubling the per-node footprint changed neither the absolute lowering cost nor
the shape of the curve. **EL does not return as a performance lever** — that is
now supported by a direct measurement rather than only by the RSS argument.

### What is left

Splitting lowering once more: **taffy node creation is flat** — 0.157 → 0.198
µs/row across 4× the rows, and only ~10% of lowering. The drift is in
`build_node`'s own per-node work, which goes 0.670 → 1.759 µs/row.

Ruled out so far: `Element` footprint, collection reallocation/rehashing, taffy
node minting, and process-wide memory pressure. What remains is per-node
allocation churn (each node builds a `desc` with cloned classes and a `to_string`
role, and the style-memo *hit* path clones a `(Style, HashMap)` pair) together
with hash-map access over tables that grow with N.

Settling which of those it is needs a profiler, and this box has none available:
`perf_event_paranoid` is 4 (blocks user-space `perf` without root) and valgrind
is not installed. That is the same class of blocker as the ARM and lavapipe
items — recorded rather than guessed at.

## Honest caveats

Stated because a competitive number is worthless without them, and because it
is easy to quote the ratio and drop the caveats.

**Against Lumen:**
* `pump()` builds the **semantics tree** every frame — the accessibility/agent
  surface. egui has no equivalent, so Lumen pays for a feature egui does not
  offer. Making it lazy is OB2, unlanded.
* Text is shaped with parley/swash (full Unicode, bidi-capable); egui's text
  layout is simpler. Different capability, different cost.

**Against egui:**
* egui is immediate-mode: no retained tree to reconcile, so it legitimately
  skips work Lumen does on purpose. A *full rebuild* is the fairest available
  comparison; comparing incremental updates would be meaningless since egui has
  none.
* egui caches galleys (laid-out text) keyed by string, so static row labels hit
  that cache every frame. This is egui's steady state, i.e. its best case.

**Neither side culls.** `ui.label` in a `CentralPanel` lays out every label
(they simply overflow), and Lumen lays out every row, so both do the full N of
work at 2000 rows despite the 400×800 viewport. The gap is not a
culling artifact.

There is an irony worth recording in that second bullet: egui's galley cache is
the *same kind of cache* serving the *same purpose*, and it is the reason egui's
steady state is as good as it is. The gap at 2000 rows was never mainly about
immediate-mode versus retained — it was one framework's string cache working and
the other's defeating itself.

## What this says about the grade

On this axis, against a same-language peer, Lumen is **not** matching the
leader, so it is not at A — let alone A+. The gap is ~6×.

But the *shape* is no longer wrong: over 500→2000 rows Lumen now scales at 5.04×
against egui's 4.99×. That distinction is the one this document originally
insisted on — "a constant-factor gap is an optimization backlog; a superlinear
one is a ceiling" — and the ceiling turned out to be a cache defect, not the
architecture. A residual gentle superlinearity remains (see *What remains*).

It does not follow that A+ is reachable cheaply, but the remaining work is
ordinary optimization, and it is now localised: **78% of a 3000-row frame is the
lowering pass**, and that is where both the constant factor and the residual
drift live (see *What remains*). (CP1's copy path has since landed; CP2 and LAY1
were retired by measurement — see the campaign record.) "Peak performance"
should still not be claimed anywhere until this table looks different.

## Next

1. ~~Find the >500-node inflection.~~ **Done** — `lumen-text` cache thrash; see
   above and the 2026-08-08 decision-log entry.
2. **Explain the residual drift inside `build_node`.** Narrowed to its per-node
   work (78% of a 3000-row frame, taffy excluded and flat). `Element` footprint,
   reallocation, taffy minting and process-wide memory pressure are all ruled
   out by measurement. **Blocked on a profiler** — no `perf` (paranoid=4), no
   valgrind. Unblock by lowering `perf_event_paranoid` or installing valgrind;
   an allocation-counting harness already exists (`benches/identity.rs`) and
   would test the allocation-churn candidate without one.
3. Extend the harness to **Slint** (the closest architectural peer — retained,
   Rust, and it has never been compared either) and **GTK4 via gtk4-rs**.
4. Add the other two owner axes: reaction latency (input → committed frame) and
   node capacity at a fixed frame budget.
