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

## Result

| rows | Lumen | egui | Lumen / egui |
|-----:|------:|-----:|-------------:|
| 100 | 395.6 µs | 37.5 µs | **10.6×** |
| 500 | 818.3 µs | 138.6 µs | **5.9×** |
| 2000 | 7 789.8 µs | 539.6 µs | **14.4×** |

## The scaling is the real finding

| | 100 → 500 (5× rows) | 500 → 2000 (4× rows) |
|---|---|---|
| **Lumen** | 2.07× | **9.52×** |
| **egui** | 3.70× | 3.89× |

egui scales linearly and predictably. Lumen scales *better* than linear up to
500 rows, then **sharply superlinear** above it — 4× the rows costs 9.5× the
time. Something changes behaviour past ~500 nodes.

That matters more than the headline ratio, because it lands directly on the
owner's third performance axis — **how many nodes can the UI hold while staying
performant**. A constant-factor gap is an optimization backlog; a superlinear
one is a ceiling.

Note this is *not* the O(scopes² × span) `copy_span` defect the CP-series
targets: this app has no `cx.scope` at all. It is a separate, unidentified
inflection, and it is the single most valuable thing this benchmark surfaced.

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

## What this says about the grade

On this axis, against a same-language peer, Lumen is **not** matching the
leader, so it is not at A — let alone A+. The gap is 6-14× and, more
importantly, the wrong shape.

It does not follow that A+ is unreachable — the campaign's unlanded work
(CP1/CP2 copy path, OB2 lazy semantics, LAY1 persistent taffy cache) all targets
exactly this frame. But "peak performance" should not be claimed anywhere until
this table looks different.

## Next

1. **Find the >500-node inflection.** Highest value; unidentified.
2. Extend the harness to **Slint** (the closest architectural peer — retained,
   Rust, and it has never been compared either) and **GTK4 via gtk4-rs**.
3. Add the other two owner axes: reaction latency (input → committed frame) and
   node capacity at a fixed frame budget.
