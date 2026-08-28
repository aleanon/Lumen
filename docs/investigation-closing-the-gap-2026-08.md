# Closing the gap to Qt and GTK — what is structural, measured

Follow-up to `report-framework-benchmarks-2026-08.md`, which put Lumen 2–9×
behind Qt and GTK on a frame where every row's text changes. This asks what
*causes* that, by measurement rather than by architecture-talk, and what would
remove it.

**The answer is narrower than "Lumen rebuilds and they don't".** The whole gap
is one thing: Lumen shapes text it never shows.

---

## 1. The gap is text shaping, and nothing else

Instrumenting the shape cache: at N = 10 000, Lumen performs **10 010 shaping
operations per frame and 19 cache hits**. Every row is shaped, every frame,
because `build_node` sizes each text node from its shaped block so taffy has a
box to lay out.

Bypassing shaping entirely (size from font metrics, `NOSHAPE=1`):

| N | with shaping | shaping bypassed | Qt | GTK |
|---:|---:|---:|---:|---:|
| 1 000 | 5 558 µs | 2 165 | 3 275 | 775 |
| 10 000 | 52 319 µs | **6 928** | 6 433 | 7 294 |

**Shaping is 87% of a 10 000-row frame.** Everything else Lumen does — the
view, lowering, taffy, paint, the meta table — already lands within noise of Qt
and GTK. There is no diffuse slowness to chase.

That also explains how Qt and GTK are fast without being magic: they do not
shape on `setText`. Shaping happens when a widget paints, and painting is
clipped to the viewport, so they shape the ~30 rows on screen. Lumen shapes all
100 000.

---

## 2. Deferred measurement — measured, and it works

A single-line label that will be **stretched by its parent** never needs its
intrinsic width, and its height is font metrics. Neither requires shaping. The
prototype (`LAZYTEXT=1`) skips shaping for exactly that case and lets paint
shape the visible rows:

| N | baseline | deferred | shapes/frame | Qt | GTK |
|---:|---:|---:|---:|---:|---:|
| 1 000 | 5 620 µs | **2 220** | 1 000 → 30 | 3 275 | 775 |
| 10 000 | 52 758 | **7 270** | 10 010 → 30 | 6 433 | 7 294 |
| 100 000 | 571 942 | **103 658** | 100 028 → 30 | 53 467 | 77 119 |

Shaping per frame collapses from *N* to **30 — the number of visible rows**,
which is precisely what Qt and GTK do. At N = 10 000 Lumen lands level with
both. At N = 1 000 it beats Qt. At N = 100 000 it is 5.5× better than before
and within 2× of Qt.

**Rendering is unchanged**: the painted output is pixel-identical (4 884 ink
pixels either way). This was checked because the first version of the prototype
was a *false win* — it skipped width unconditionally, the content-sized parent
then collapsed to zero width, nothing was drawn at all, and the frame time
looked like 471 µs. `ink_pixels = 0` caught it.

### What it costs

The transformation is only valid when the node's width is decided by its
parent. Run naively across the corpus it **fails 121 of 1 173 tests**, because
most test views have a content-sizing ancestor, where the intrinsic width is
genuinely consumed. A shippable version needs the guard — "does any ancestor
content-size on this axis" — which is the same definite/indefinite
containing-block question L1's rejected workaround ran aground on, and it must
be answered during lowering rather than after.

Two smaller corrections the prototype also needs: line height must come from
the font's real metrics (the crude `1.2 × font_size` gave 20 px where the true
value is 21), and a stretched row's reported width changes from the glyph
advance to the parent's width — which is what CSS `align-items: stretch`
prescribes, but is a visible behaviour change.

---

## 3. What is left after that, and what it would take

At N = 100 000 the deferred-measurement frame is 104 ms, about **1 µs per
node**, and it is now ordinary lowering rather than shaping. Options, in
descending value:

| | change | estimated | cost |
|---|---|---|---|
| **A** | **Deferred text measurement** (above) | **−86%** at N=10k, measured | needs the definite-width guard; 121 tests to satisfy |
| B | Viewport culling of layout and paint | large at big N | offscreen nodes must still exist for the semantics tree (principle 2), so only layout/paint can be skipped, and layout is what tells you they are offscreen — needs incremental top-down layout that stops once past the viewport |
| C | Parallel shaping | ~8–16× on shaping | only matters where A does *not* apply; shaping is pure, so determinism (ADR-002) holds |
| D | Direct lowering (O0.16–O0.24) | −18% of lowering ≈ 9–11% of frame | the migration, already costed |
| E | Retained/dirty subtrees | the remaining architectural gap | Lumen already has scope memoisation; this benchmark defeats it by changing every row. A frame changing 1% of rows does not pay this at all |

**A is the one to do.** It is measured, it lands Lumen level with Qt and GTK at
10 000 nodes, and it is a bounded change to one function rather than an
architectural rewrite. B is the natural follow-on and is what would close the
100 000-node case.

---

## 4. The honest framing

The benchmark deliberately measures the worst case: *every* row changes, which
defeats memoisation and forces a reshape everywhere. Real frames change a small
fraction of their rows, and there Lumen's memoisation already applies and this
gap is much smaller.

But the finding stands on its own terms, because it is not about rebuilding at
all: **Lumen does O(N) text shaping where Qt and GTK do O(visible)**, and that
is true of every frame, memoised or not, whenever the text is new.

---

*Method: shape counts from an instrumented `shaped_by_key`; the bypass and
deferred paths are env-gated diagnostics, since reverted. Correctness of the
deferred path checked by comparing painted ink pixels and the semantics tree,
not by inspection.*
