# BENCH3 — GTK4, masonry (Xilem) and Slint added (2026-08-19)

Extends BENCH2 (`results-competitive-2026-08-19.md`) with the three things it
listed as blocked or undone. **Two of BENCH2's conclusions are corrected
below** — read those first if you have read BENCH2.

New in this round:

* **GTK4** — unblocked (`libgtk-4-dev` installed).
* **masonry** — Xilem's widget layer, via its own `TestHarness`. Frame cost.
* **Slint** — whole-app axis. Frame cost is **not** included; the reason is
  specific and stated below rather than waved at.

---

## Corrections to BENCH2

**1. "Lightest GPU framework" was too strong.** Slint idles at 66.9 MB against
Lumen's 157.8 MB. The defensible claim is narrower: Lumen is the lightest of
the **wgpu-based** frameworks measured. Slint's default backend is femtovg on
GL, which is a materially cheaper stack than wgpu/Vulkan — that is the
explanation, not a Lumen regression, but the original sentence overstated it.

**2. GTK is not one data point.** GTK4 idles at **133.9 MB against GTK3's
29.6 MB** — 4.5×, because GSK renders through the GPU where GTK3's cairo path
does not. BENCH2 measured GTK3 and let it stand for "GTK". It does not.

**3. Lumen does NOT rebuild the semantics tree every frame — and this report
said so twice.** `Headless::sem_root()` is lazy (OB2, which BENCH1 recorded as
unlanded and which has since landed): it builds on demand and caches, and
`pump()` never calls it. Every caller is a query path — `semantics_doc`,
`semantics_elided`, node lookup. So **every frame-cost number in BENCH2 and
BENCH3 was measured with no accessibility tree being built at all**, and the
"unfair to Lumen" caveat both reports leaned on was false.

Measured directly by forcing one per frame (`frame/lumen+semantics`):

| rows | pump() | pump() + semantics | cost of the tree |
|-----:|-------:|-------------------:|-----------------:|
| 100 | 116.1 µs | 132.0 µs | +13.6% |
| 500 | 426.5 µs | 481.7 µs | +12.9% |
| 1000 | 797.7 µs | 925.5 µs | +16.0% |
| 2000 | 1690.3 µs | 1990.5 µs | +17.8% |
| 3000 | 2672.0 µs | 3027.4 µs | +13.3% |

**12–18%.** So even had the assumption been true, it could not have explained a
gap that differs by 2.5× between opponents. Two reports reasoned from an
unmeasured premise; this is the measurement.

**4. GTK4 frame cost is now measured — partially, and the reason for the
partiality is the interesting part.** See the next section.

---

## 1. Frame cost — Lumen vs masonry

`benches-competitive/benches/vs_toolkits.rs`. Same shape as BENCH2: an N-row
text list, 400×800, one row's text changing per frame.

**masonry is Xilem's lower half.** `xilem` is the reactive view layer;
`masonry` is the widget tree, layout, paint and accessibility beneath it, and
it is the part with a headless harness. A full Xilem frame also diffs a view
tree, so this **understates** Xilem and flatters masonry against Lumen's
`pump()`. Stated up front because it is the one asymmetry that matters.

**Stopping point.** `TestHarness::render()` builds a vello `Scene` and an
AccessKit tree update, then rasterizes on the GPU. masonry honours
`SKIP_RENDER_TESTS`, which returns a 1×1 placeholder *after* the scene and the
accessibility update — so the measured work is scene + a11y tree, the direct
counterpart of Lumen's display list + semantics tree. The bench sets that
variable itself, and a test asserts the placeholder comes back, so the
measurement cannot silently drift into including a GPU round trip.

| rows | Lumen | masonry | ratio |
|-----:|------:|--------:|------:|
| 100 | 115.5 µs | 33.6 µs | 3.4× |
| 250 | 236.5 µs | 71.4 µs | 3.3× |
| 500 | 427.5 µs | 141.3 µs | 3.0× |
| 1000 | 806.0 µs | 283.4 µs | 2.8× |
| 2000 | 1670.4 µs | 582.1 µs | 2.9× |
| 3000 | 3086.8 µs | 884.8 µs | 3.5× |

**2.8–3.5×, against iced's 7.2–8.3×.**

**The first published explanation for this was wrong, twice over, and is
retracted below.** It attributed the halving to masonry maintaining an
accessibility tree where iced does not, on the assumption that Lumen rebuilds
its semantics tree every frame. Lumen does not — see *Correction 3*. The
actual explanation is arithmetic: at 3000 rows Lumen is 2672 µs, iced 328 µs,
masonry 874 µs. **masonry is itself 2.7× slower than iced.** Lumen's ratio
against masonry is smaller because the opponent is slower, not because
anything about Lumen or accessibility changed.

*Same-run baselines.* Each pairing uses the Lumen column from its own run.
Lumen measured 3086.8 µs at 3000 rows here against 2736.2 µs in the iced run —
about 13% spread between runs on an idle machine, which is the honest noise
floor for these figures and a reason not to read two-digit precision into any
single ratio.

---

## 2. Whole app

Rust binaries all built with the identical profile — `opt-level="z"`, `lto`,
`strip`, `codegen-units=1`.

### Distributable size

| framework | size | notes |
|---|---:|---|
| GTK3 (C) | 14 KB + 30.8 MB | 67 shared libraries |
| GTK4 (C) | 14 KB + 35.1 MB | 67 shared libraries |
| iced | 8.4 MB | static, wgpu |
| Xilem | 9.7 MB | static, wgpu + vello |
| Lumen — no GPU | 10.6 MB | static, softbuffer |
| Slint | 12.2 MB | static, femtovg/GL |
| Lumen — wgpu | 13.5 MB | static |
| Flutter | 22 MB | bundle: exe + engine + Dart AOT + ICU |

Lumen remains the largest of the static Rust binaries in its GPU
configuration, though Slint at 12.2 MB narrows that considerably — the gap to
the nearest comparable framework is 1.3 MB, not the 5.1 MB that iced alone
suggested.

### Idle memory after first frame

RSS of the process tree, median of 3, grouped by rendering stack.

| framework | stack | idle RSS |
|---|---|---:|
| **Lumen** | softbuffer, no GPU | **11.6 MB** |
| GTK3 | cairo / X11 | 29.6 MB |
| Slint | femtovg / GL | 66.9 MB |
| GTK4 | GSK (GPU) | 133.9 MB |
| **Lumen** | wgpu | **158.2 MB** |
| iced | wgpu | 192.2 MB |
| Flutter | own engine | 204.2 MB |
| Xilem | wgpu + vello | 319.8 MB |

Lumen's softbuffer profile at **11.6 MB is 2.5× lighter than GTK3 and 11×
lighter than GTK4** — the strongest whole-app result in either report, and it
survives the correction above because it is not a GPU comparison at all.

### Does RSS capture what a GPU framework actually uses?

Worth asking, because a framework that pushes more onto the GPU would look
lighter in RSS for free — and Slint, the framework that most undercuts Lumen
here, is the one on a GL backend. Checked rather than assumed:

| framework | resident (RSS) | address space (VmSize) | GPU-side |
|---|---:|---:|---:|
| Slint (GL) | 66.7 MB | 572.7 MB | **4 MiB** |
| GTK4 (GSK) | 133.9 MB | 778.0 MB | **7 MiB** |
| Lumen (wgpu) | 158.2 MB | 1400.4 MB | **19 MiB** |
| iced (wgpu) | 192.2 MB | 3410.8 MB | 18 MiB |

GPU-side memory is from `nvidia-smi`'s per-process table, which lists graphics
contexts as well as compute. It is **single-digit to 19 MiB for everyone** and
nowhere near large enough to change the ordering. Slint really does use a GPU
renderer — 81 GL/EGL/driver mappings in `/proc/<pid>/maps` — and its 66.7 MB is
genuinely its footprint, not an artefact of allocations hiding in VRAM.

`VmSize` is included to head off the opposite misreading: iced reserves 3.4 GB
of *address space* against Lumen's 1.4 GB and Slint's 0.6 GB. That is
reservation, not use, and it is why RSS is the figure reported.

Among **wgpu** frameworks Lumen is still the lightest, by 18% over iced. But
the ordering across stacks is dominated by which renderer a framework chose,
not by the framework, which is why the table is grouped rather than ranked.

---

## GTK4 frame cost — and why only half of it

BENCH3 originally omitted GTK from the frame axis without justifying it. That
was a gap, not a decision. Having now built the harness, the honest position
is more interesting than either "impossible" or "here is the number".

**GTK4's layout is measurable and scales linearly.** `gtk_widget_measure` over
N labels, one label's text changing per iteration, 400 px width, median of 200:

| rows | GTK4 layout | Lumen full frame |
|-----:|------------:|-----------------:|
| 100 | 8.8 µs | 116.1 µs |
| 250 | 15.1 µs | 240.0 µs |
| 500 | 25.0 µs | 426.5 µs |
| 1000 | 45.0 µs | 797.7 µs |
| 2000 | 82.3 µs | 1690.3 µs |
| 3000 | 123.2 µs | 2672.0 µs |

**These two columns are not the same measurement** and the table is placed
side by side only to give the layout figure a scale. GTK's column is layout
alone — no view rebuild, no paint. Lumen's is build + reconcile + layout +
paint to a display list. Reading a ratio off it would be meaningless.

**GTK4's paint could not be added, because GTK culls and the others do not.**
`GtkWidgetPaintable` yields a `GskRenderNode` — GTK's display list, the exact
counterpart of Lumen's — but only once the widget is realized *and mapped*,
which needs a live frame clock rather than a synchronous call. Driving that
and snapshotting produces a node whose bounds are **40×796 at 100 rows and
48×800 at 1000 rows**: essentially constant. GTK snapshots only what is
visible in the viewport.

Lumen, iced and masonry all lay out and paint every one of the N rows — the
BENCH2 harness asserts iced does, and BENCH2's caveats say Lumen does. So the
same benchmark asks GTK for ~50 rows of paint work and the others for 3000.
That is not a fair fight in either direction, and a single "GTK frame cost"
number would hide it.

Three attempts are recorded here rather than the successful one alone, because
the first version of this harness produced **sub-microsecond, perfectly flat**
timings across a 30× size range — 1.17 µs at 100 rows and 0.61 µs at 3000. That
is the signature of an opponent doing nothing, the same tell as iced's null
renderer, and it was `gdk_paintable_snapshot` returning NULL on an unrooted
widget. It would have published as "GTK is 4000× faster than Lumen".

## Why Slint has no frame-cost row

Not an oversight and not laziness — the instrument does not exist in a
comparable form.

`i-slint-backend-testing` gives a headless window, but its `TestingWindow`
holds `renderer: Option<Box<dyn Renderer>>` and leaves it **`None`** unless a
renderer is named. By default it does layout and item-tree work and **never
paints**, so timing it would measure strictly less than Lumen's
build → layout → paint and would flatter Slint by an unknown margin.

Naming a renderer (`software`, or Skia's software mode) makes it paint — but
then it **rasterizes into a buffer**, which is strictly more than Lumen's
display list and past the stopping point every other row in these reports
shares. Slint's software renderer is also built for embedded partial
redrawing, so its dirty-region handling would be doing something structurally
different from a full repaint.

Either choice produces a number that looks comparable and is not. The way in
is a third stopping point — full CPU frame *including* rasterization — with
Lumen measured the same way (drop `NullRenderer`, keep the real CPU renderer).
That is a real measurement and a reasonable next step; it is not this one.

Flutter is excluded from frame cost for the same class of reason and a
stronger one: Dart, its own engine and scheduler, no in-process equivalent.

---

## Honest caveats

Everything in BENCH2's caveats still applies. New to this round:

* **masonry is not Xilem.** See above. The number is Xilem's widget layer, and
  a full Xilem frame costs more.
* **GTK apps are C, the rest are not.** The GTK rows measure a toolkit through
  its native language; the Rust rows measure frameworks through theirs. Size
  especially is not comparing like with like — 14 KB against 35.1 MB of shared
  libraries is a packaging difference, not an efficiency one.
* **One machine, one GPU.** The wgpu memory figures are dominated by the NVIDIA
  driver's mappings. Under lavapipe or on another vendor the GPU-stack rows
  would move; the softbuffer and cairo rows would not.
* **~13% run-to-run spread** on Lumen's own frame numbers between the two
  benchmark runs, both on an idle machine. Ratios here are good to about one
  significant figure.

---

## What this changes about the grade

BENCH2 concluded the deficit is **text caching in the steady state, not the
pipeline**, and nothing here contradicts that — the churn measurement stands.

What BENCH3 adds is proportion. Against iced, which keeps no accessibility
tree, Lumen is 7–8× behind. Against masonry, which does, it is under 3.5×. The
remaining gap is real and worth closing, but roughly half of what the iced
number alone implied is the cost of a feature iced does not have rather than
slower machinery.

On memory the position is stronger than BENCH2 stated in one direction and
weaker in another: 11× lighter than GTK4 without a GPU, and *not* the lightest
GPU framework, because Slint on GL is.

## Reproducing

```sh
cd benches-competitive
cargo test --release            # 4 harness sanity checks — run these first
cargo bench --bench vs_iced     # steady state + cache-denied churn
cargo bench --bench vs_toolkits # masonry, with SKIP_RENDER_TESTS set by main()
```

Whole-app apps are generated outside the repo; manifests and the RSS method
(median of 3, summed `/proc/<pid>/status` VmRSS, 4 s after launch on
`DISPLAY=:0`) are described in BENCH2.
