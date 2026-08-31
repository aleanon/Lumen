# BENCH5 — two workloads, GTK4 and Qt6, frametime + memory + stages (2026-08-30)

Adds to BENCH4 (`results-competitive-bench4.md`) rather than replacing it.
BENCH4 measured one workload (a 3000-row list with one row changing) against
six toolkits. This round asks the two opposite questions separately, against the
two retained-mode C/C++ toolkits, and measures memory and per-stage cost for
both:

* **`point`** — N rows, **one** row's text changes per frame. Does a framework's
  cost track the *change* or the *size of the tree*?
* **`churn`** — N rows, **every** row's text changes per frame. Nothing is
  reusable: no scope memo, no shaped-text cache, no spliced span. Raw
  throughput.

N sweeps 100 / 1000 / 3000 / 10000. Everything is `taskset -c 2` on the same
i9-13900KF, minimum of 200 (point) or 50 (churn) iterations after warm-up, best
of 3 repetitions. Harnesses are in `benches-competitive/harnesses/bench5/`;
`run.sh` reproduces every number below.

---

## 0. The headline is a regression, and it is in Lumen's default build

**A default-feature Lumen app is now 112× slower per frame at 3000 rows, and
304× slower at 10000 rows, than the same code built without default features.**

| n | `--no-default-features` | default build | ratio |
|---:|---:|---:|---:|
| 100 | 41.7 µs | 178.8 µs | 4.3× |
| 1 000 | 61.0 µs | 2 142 µs | 35× |
| 3 000 | 93.1 µs | 10 439 µs | 112× |
| 10 000 | 284 µs | 86 325 µs | **304×** |

*(patch path, `point` workload — a one-row text change.)*

The cause is not subtle once looked for: `dev-observability` is **default-on**
in `lumen-widgets`, and `pump()` calls `ambient_audit()` on **every painted
frame** (`crates/lumen-app/src/app.rs:1638`), which runs a full `lint()` over
the whole tree. `perf --children` puts **71.1%** of a 3000-row default-build
frame inside that one call.

Two things make this worth flagging rather than filing:

* **BENCH4's numbers are still correct, and they are release-profile numbers.**
  Rebuilding BENCH4's own unchanged probe here reproduces its published table to
  within 1.5% (171.7 µs vs 174.3 published rebuild; 51.1 vs 51.0 patch) — but
  *only* with default features off. The comparison table everyone reads
  describes a build most consumers will not get by default.
* **The design is deliberate and the default is arguable; the cost is not.** The
  audit is gated to painted frames and edge-triggers its logging, which is
  careful. What it does not do is scale with *what changed* — it is O(tree) on
  every painted frame, so it costs 86 ms on a 10 000-row list where the frame
  itself costs 0.28 ms. That is 11 fps for a build a developer gets from
  `cargo run`.

Everything below reports the release profile as "Lumen", with the default build
shown separately where it matters. Comparing a per-frame linter against GTK and
Qt, which run no such pass, would not be a comparison.

---

## 1. Frametime — `point` (large tree, one row changes)

Change plus layout, no rasterisation, for all three. Lumen stops at the display
list; Qt's row is `setText` + `invalidate` + `activate`; GTK's is
`gtk_label_set_text` + `gtk_widget_measure`.

| n | **Lumen patch** | Lumen rebuild | GTK4 | Qt6 |
|---:|---:|---:|---:|---:|
| 100 | 41.7 | 66.1 | **8.8** | 13.5 |
| 1 000 | 61.0 | 325.3 | **40.3** | 67.1 |
| 3 000 | **93.1** | 850.6 | 110.8 | 209.6 |
| 10 000 | **283.9** | 3 550.4 | 396.5 | 706.9 |

*µs, minimum of 200 iterations, best of 3 runs.*

**The slope is the result.** Fitting the 1 000 → 10 000 secant:

| | µs per row | fixed cost |
|---|---:|---:|
| **Lumen — patch** | **0.025** | ~36 µs |
| GTK4 | 0.040 | ~1 µs |
| Qt6 | 0.071 | ~0 µs |
| Lumen — rebuild | 0.358 | ~0 µs |

Lumen's patch path has the flattest curve of anything measured — **6.8× across a
100× increase in rows**, against GTK's 45× and Qt's 52×. It pays for that with a
~36 µs floor neither C toolkit has, so it loses below ~1 500 rows and wins above:
it passes Qt between 100 and 1 000 rows, and GTK between 1 000 and 3 000.

**Neither retained toolkit does incremental layout.** Qt relayouts the entire box
for one changed label (`activate()` is 707 µs at 10 000 rows against 1.2 µs for
the `setText` itself), and GTK re-measures the whole box. Retained-mode buys
them a low fixed cost, not O(changed) work — which is the thing Lumen's patch
path actually has.

## 2. Frametime — `churn` (every row changes)

| n | **Lumen patch** | Lumen rebuild | GTK4 | Qt6 |
|---:|---:|---:|---:|---:|
| 100 | 1 468 | 1 531 | **460** | 688 |
| 1 000 | 5 137 | 5 701 | **4 678** | 6 871 |
| 3 000 | **13 361** | 16 096 | 14 370 | 20 665 |
| 10 000 | **45 843** | 60 623 | 49 822 | 68 953 |

*µs, minimum of 50 iterations, best of 3 runs.*

Same shape, weaker. Per row: Lumen patch **4.52 µs**, GTK **5.02**, Lumen
rebuild **6.10**, Qt **6.90**. Lumen has the lowest marginal cost again but
carries a ~614 µs fixed cost, so it trails GTK to about 2 000 rows and leads
past 3 000. Qt is last at every size from 1 000 up.

Nothing here runs at 60 fps. **At 3 000 rows, changing every label costs
13–21 ms in every framework measured** — a frame budget entirely spent. That is
the honest read: full churn of a large list is not a frame-rate workload for any
of them, and the differences between 13 ms and 21 ms matter much less than the
fact that all three are over budget.

## 3. GTK's real end-to-end frame — and a correction to BENCH3/BENCH4

BENCH3 and BENCH4 both stated that GTK4's render node is unreachable
synchronously because `gtk_widget_snapshot_child` "returns NULL", and gave GTK no
paint row on that basis. **That is wrong.**
`gtk_widget_paintable_new` + `gdk_paintable_snapshot` returns a perfectly good
`GskRenderNode`. It is a **stale** one: serializing it before and after changing
a label gives byte-identical output (776 404 bytes both times), and only after
pumping the main loop does the serialization differ. GTK caches each widget's
render node and rebuilds it in the frame clock's layout phase.

So the conclusion survives with a different reason, and one more consequence:
the rebuild cannot be isolated by timing the pump either, because the frame
clock is vsync-gated and a minimum-of-N selects precisely the pumps that did no
work. `harnesses/bench5/gtk.c` therefore measures GTK's paint the only honest
way available — **sustained throughput**, counting real frame-clock ticks over a
3-second window:

| n | point | churn |
|---:|---:|---:|
| 100 | 21.1 ms | 19.9 ms |
| 1 000 | 23.1 ms | 19.0 ms |
| 3 000 | 22.7 ms | 24.6 ms |
| 10 000 | 25.6 ms | **63.8 ms** |

The 60 Hz floor is 16.67 ms, so everything up to 3 000 rows is roughly
vsync-bound and says only "cheaper than this". The 10 000-row churn row is not:
**GTK's real frame there is 63.8 ms (15.7 fps)**, against the 49.8 ms its
synchronous `measure` row reports. The difference — ~14 ms — is the render-node
build, GSK render and present that the synchronous instrument cannot see.

**This is the only end-to-end row in this document.** Every other number stops
before rasterisation.

## 4. Where the frame goes

Qt and GTK time their own stages directly. Lumen's stages are private functions
inside `pump()`, so its split comes from `perf --children` inclusive attribution
(2 kHz, DWARF unwinding) — non-invasive, using the debuginfo the release profile
already emits. **The percentages nest**: `text-shape` sits inside
`lower(build_node)`, and `display-list` sits inside `patch-bindings`.

### Lumen, n = 3000

| stage | point/rebuild | point/patch | churn/rebuild | default build, point/patch |
|---|---:|---:|---:|---:|
| build closure | 19.6% | 1.4% | 8.3% | 0.0% |
| lower (`build_node`) | 19.6% | 25.0% | **68.1%** | 0.3% |
| ⤷ text shaping | 4.4% | 21.8% | **61.8%** | 12.7% |
| layout (taffy) | **33.1%** | 3.7% | 3.2% | 0.0% |
| display list | 11.2% | 54.1% | 6.7% | 1.0% |
| patch bindings | — | **55.0%** | — | 1.2% |
| **ambient audit (`lint`)** | 0.0% | 0.0% | 0.0% | **71.1%** |

Three readings:

* **A rebuild frame is a layout frame.** taffy is a third of it; text shaping is
  4%, because the memo means only the changed row re-shapes.
* **A churn frame is a text-shaping frame.** 61.8% of it is inside parley/swash,
  and taffy drops to 3%. No layout or reconciliation optimisation moves this
  number; only shaping or caching does.
* **A patch frame is a display-list frame.** The whole frame is "re-shape one
  string, re-emit the culled display list". This is why it is nearly flat in N —
  emission is culled to the ~50 visible rows regardless of tree size.

### Qt, direct stage timings (µs)

| n / mode | `setText` | + layout | + paint | idle floor |
|---|---:|---:|---:|---:|
| 3 000 point | 1.2 | 209.6 | 542.0 | 277.0 |
| 3 000 churn | 979.1 | 20 664.6 | 21 012.2 | 355.7 |
| 10 000 point | 1.2 | 706.9 | 1 400.7 | 472.9 |
| 10 000 churn | 3 350.9 | 68 952.9 | 69 792.8 | 567.1 |

Qt's `setText` is essentially free; the cost is entirely the relayout it
triggers. Its rasterisation of the 400×800 viewport (`+paint` minus `+layout`)
is 330–700 µs, which alone exceeds Lumen's entire patch frame at every size —
and no Rust framework in this comparison rasterises at all, so that column is
context, not a competitor.

`perf` at the shared-object level agrees with both: **Qt churn is 41% harfbuzz**,
**GTK churn is 27% harfbuzz + 24% pango**. All three frameworks spend the churn
frame in text shaping. That is the real finding of the churn workload — it is a
text-engine benchmark wearing a UI-framework costume.

## 5. Memory

`VmRSS` / `VmHWM` from `/proc/self/status`, read by each harness itself, so C,
C++ and Rust report from the same source in the same units.

**Per-row cost**, from the 1 000 → 10 000 secant (a secant cancels the fixed
toolkit footprint, which at n = 100 completely swamps the rows):

| | point | churn |
|---|---:|---:|
| GTK4 | **4.87 KB/row** | 5.98 KB/row |
| Lumen — patch | 6.05 KB/row | **3.79 KB/row** |
| Lumen — rebuild | 8.61 KB/row | 13.04 KB/row |
| Qt6 | 29.45 KB/row | 29.50 KB/row |

**A `QWidget` costs ~29 KB of RSS per row — 6× a `GtkLabel` and 5× a Lumen text
node**, and it does not vary with workload. This is the clearest single result in
the memory data.

**Peak RSS for the whole run at n = 10 000:**

| | point | churn |
|---|---:|---:|
| Lumen — patch | **72 MB** | 92 MB |
| Lumen — rebuild | 91 MB | 211 MB |
| GTK4 | 191 MB | 196 MB |
| Qt6 | 317 MB | 318 MB |

**Read this one carefully.** Lumen here runs headless on a null renderer with no
window and no GPU stack; GTK runs a real window on GSK/NVIDIA; Qt runs offscreen
raster. The *per-row* table above is comparable; this absolute table is not a
whole-app comparison. BENCH4's idle-RSS table (real windows, all frameworks) is
the one to quote for that, and it puts Lumen at 162 MB against Qt's 46 MB.

What *is* comparable and worth noting: **Lumen's rebuild path allocates 3.4× the
per-row memory of its patch path under churn** (13.04 vs 3.79 KB/row), and peaks
at 211 MB against 92 MB. The retained arena churns hard when every span is
invalidated.

---

## Caveats

* **Not all rows stop at the same place.** Lumen stops at the display list;
  GTK's and Qt's primary rows stop after layout; only GTK's sustained-throughput
  table is end-to-end. Ratios are meaningful only within a column.
* **`point` at small N flatters the C toolkits and `churn` at small N flatters
  them more.** Lumen's fixed per-frame cost (~36 µs point, ~614 µs churn) is
  real and is the thing to attack if small-list latency matters.
* **The churn workload is dominated by text shaping in all three frameworks.**
  It is a good stress test and a poor discriminator of framework architecture.
* **One box, one driver, one font.** Linux/X11, i9-13900KF, RTX 4070, 60 Hz.
* **Qt's `offscreen` platform plugin** warns `propagateSizeHints()` unsupported;
  it does not affect layout timings, which are computed in-process.

## Reproducing

```sh
cd benches-competitive
OUT=/tmp/bench5 ./harnesses/bench5/run.sh > bench5.tsv    # full matrix
N=3000 OUT=/tmp/bench5 ./harnesses/bench5/profile.sh      # stage attribution
```

`run.sh` builds both Lumen profiles, the GTK4 harness and the Qt6 harness, then
runs the whole (framework × mode × N) grid. It warns if another process is over
50% CPU — during this round a background process at 100% moved a reading by 38%
before the guard was added.
