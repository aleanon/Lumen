# Lumen vs Qt, GTK, iced and Xilem/Masonry — 2026-08-29

Five UI frameworks, one workload, one machine. What Lumen costs against the
alternatives, where each one spends its time, and which of the differences are
architectural rather than incidental.

**Machine.** i9-13900KF (32 threads), 62 GB, X11 on `:0`, NVIDIA present.
**Versions.** Lumen @ `exp/widget-trait`, Qt 6.4.2, GTK 4.14.5, iced 0.14.2
(local checkout), Masonry 0.4.0 (the widget layer Xilem builds on).

---

## The workload

A vertical list of **N rows**, each a text label reading `row <i> · <counter>`,
optionally wrapped in **D** nested vertical containers. Two measurements:

* **build** — construct the UI from nothing and produce one frame.
* **changed frame** — increment the counter so **every row's text changes**,
  then produce a frame. Min and median of 40 frames after 15 warm-up frames,
  one process per data point.

Every label's text changes on purpose. An earlier Lumen-only benchmark held the
text constant, which let the shaping cache hit on every node and measured
something no real frame does. Changing it makes shaping real and, more
importantly, makes it *equal*: no framework can serve a new string from a cache.

**Rasterisation is CPU in every arm** — Lumen headless, iced via `tiny_skia`,
Masonry via its test harness, Qt via `QWidget::grab`, GTK via
`gsk_cairo_renderer`. No arm gets a GPU advantage.

---

## Frame time (µs, min of 40), depth 0

| N | GTK | Qt | **Lumen** | **Lumen lean** | iced | Masonry |
|---:|---:|---:|---:|---:|---:|---:|
| 100 | 125 | 544 | 2 006 | 1 958 | 1 802 | 16 500 |
| 1 000 | 775 | 3 275 | 5 610 | 5 167 | 9 605 | 32 703 |
| 10 000 | 7 294 | 6 433 | 51 679 | 48 566 | 92 978 | *timeout* |
| 100 000 | 77 119 | 53 467 | 564 527 | — | 969 622 | *timeout* |

**Lumen is 2–9× slower than the retained toolkits and 1.7–1.9× faster than
iced.** Masonry is far behind, with the caveat below.

### Depth (N = 1000)

| D | nodes | GTK | Qt | Lumen | iced |
|---:|---:|---:|---:|---:|---:|
| 0 | 1 001 | 775 | 3 275 | 5 610 | 9 605 |
| 4 | 5 001 | 807 | 4 111 | 15 305 | 10 161 |
| 8 | 9 001 | 1 603 | 4 867 | 28 407 | 10 842 |

Nesting costs Lumen roughly linearly in *nodes* (5 610 → 28 407 for 9× the
nodes). iced barely moves — its containers are cheap and its cost is dominated
by text. Qt and GTK barely move because they do not rebuild.

---

## Where the time goes

Measured inside each harness, not inferred.

| framework | phase | N=1000 | N=10000 |
|---|---|---:|---:|
| **Lumen** | view (build `Element`s) | 77 | 1 131 |
| | **build_node (lower + shape)** | **4 056** | **49 400** |
| | layout (taffy) | 152 | 2 081 |
| | paint (CPU raster, 400×600) | 1 492 | 1 903 |
| | observability audit | ~0 | ~0 |
| **iced** | **view + layout** | **8 705** | **92 173** |
| | draw (primitives) | 24 | 911 |
| | raster | 985 | 1 278 |
| **Qt** | setText (× N) | 300 | 2 761 |
| | **layout + render** | **3 003** | **3 756** |
| **GTK** | set_text (× N) | 265 | 2 776 |
| | render | 513 | 4 715 |
| **Masonry** | edit (batched) | 2 349 | — |
| | **render** | **33 930** | — |

`perf` on the steady state agrees about the cause: Qt is **43% Qt6Gui + 22%
harfbuzz**, GTK is 22% glib + 6.6% harfbuzz + 6.4% fontconfig. **Text shaping
is a first-order cost in every framework here**, which is why the workload was
built to make it equal.

For Lumen, `build_node` is **70% of the frame at N=1000 and 88% at N=10000**,
and it is shaping-bound. `audit ≈ 0` confirms the O0.15 throttle: the
observability pass, which was 27% of a frame before it, no longer registers.

---

## Binary size

| framework | binary | toolkit shared libs | total |
|---|---:|---:|---:|
| iced (tiny-skia) | 3.21 MB | — | **3.21 MB** |
| **Lumen lean** | 4.41 MB | — | **4.41 MB** |
| Lumen full | 8.41 MB | — | 8.41 MB |
| Masonry | 9.13 MB | — | 9.13 MB |
| GTK | 18 KB | 19.7 MB | 19.7 MB |
| Qt | 22 KB | 22.4 MB | 22.4 MB |

Qt and GTK binaries are stubs; the toolkit is 19–22 MB of shared library. That
is free if the platform ships it and a 20 MB dependency if it does not — the
comparison is a deployment question, not a size one.

**Lumen lean is the like-for-like number.** It is `--no-default-features`: CPU
rasteriser, no GPU backend, no accessibility bridge, no image codecs, no
agent/snapshot surface, no per-frame audit — the capability set the iced
harness has. Halving 8.41 → 4.41 MB is what the feature flags are *for*, and it
costs almost nothing in frame time (5 610 → 5 167 µs), because O0.15 had
already made the observability tier nearly free.

---

## Memory (peak RSS, kB)

| N | iced | Lumen lean | Lumen | Qt | GTK | Masonry |
|---:|---:|---:|---:|---:|---:|---:|
| 100 | 7 900 | 15 968 | 16 240 | 48 720 | 138 816 | 347 412 |
| 1 000 | 14 100 | 24 720 | 25 000 | 49 068 | 143 648 | 327 968 |
| 10 000 | 77 312 | 109 716 | 117 004 | 70 036 | 189 776 | — |
| 100 000 | 729 724 | — | 408 344 | 278 876 | 654 800 | — |

Lumen sits between iced and the C/C++ toolkits at small N, and **beats iced by
1.8× at 100 000 nodes** (408 MB vs 730 MB). Qt is the most economical at scale.

---

## What is not comparable, and why

Stated rather than buried, because three of these nearly produced false results.

1. **GTK does not lay out the whole list.** Its box clamps to the window
   (400×1008 whatever N is), so it rasterises ~63 rows at every N. Its `set`
   phase scales with N and its render phase does not. Lumen, iced and Masonry
   lay out all N; Qt lays out all N and originally *expanded its window* to
   15 000 px and rasterised every row — 12× the pixels — until it was pinned
   with `setFixedSize`. GTK's frame numbers are therefore a floor, not a
   like-for-like.
2. **Masonry is measured through `masonry_testing::TestHarness`**, the only
   headless synchronous driver it exposes. It is a *test* harness and may carry
   validation overhead. The first version of this benchmark also used
   `edit_widget_with_id` per row, which runs `process_signals()` after **every**
   call — 81 µs per `set_text`, and a reported figure 4.5× worse than the truth.
   Batching into one `edit_root_widget` fixed it. Treat Masonry's numbers as an
   upper bound on its cost.
3. **The GTK harness initially reported a 0 µs render** and a 36× advantage
   over Qt that did not exist: a `GtkWidgetPaintable` over an unmapped widget
   snapshots to `NULL`, and the harness cheerfully timed nothing. It now
   asserts that a node and a texture were produced and exits non-zero
   otherwise.

Every one of those was caught by a number being implausible rather than by
suspicion. The harnesses now fail loudly instead of reporting quietly.

---

## Reading it

**Retained beats rebuilt when everything changes, but not by architecture
alone.** Qt and GTK win at large N because a text change dirties a widget
rather than rebuilding a tree. Lumen and iced rebuild the view every frame; at
N=100 000 that is 0.56 s and 0.97 s respectively. Lumen's answer to this is
scope memoisation, which this workload deliberately defeats by changing every
row — a real app changing 1% of its rows would not pay that.

**Lumen's remaining cost is concentrated, not diffuse.** 88% of a 10 000-row
frame is `build_node`, and it is shaping-bound. That is one place to attack,
not a general slowness.

**Lumen is the second-smallest binary and the second-least memory-hungry**, and
is the only one here whose footprint is *tunable* — 8.41 → 4.41 MB by feature
flags, for a 7% frame-time cost.

**Against iced specifically** — the closest architectural comparison, both Rust,
both rebuild-per-frame, both CPU-rasterised — Lumen is **1.7–1.9× faster on
frame time and 1.8× lighter at 100 000 nodes**, and 1.2 MB larger in the lean
configuration.

---

## Reproducing

Harnesses in `scratchpad/fwbench/` (`SPEC.md`, `run.sh`, one directory per
framework); Lumen's arm is `benches/src/bin/fwbench.rs`. One process per data
point — no arm shares an allocator or a warm cache with another.
