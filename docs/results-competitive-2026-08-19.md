# BENCH2 — Lumen vs iced, Xilem, GTK3 and Flutter (2026-08-19)

> **Two claims here are corrected by `results-competitive-bench3.md`
> (BENCH3):** "lightest GPU framework measured" is too strong — Slint idles at
> 66.9 MB against Lumen's 157.8, so the defensible claim is *lightest of the
> wgpu-based* frameworks; and the GTK3 row does not stand for "GTK", since
> GTK4 idles at 133.9 MB, 4.5x GTK3. BENCH3 also adds masonry frame cost,
> where the gap is under 3.5x rather than iced's 7-8x.

Successor to `results-competitive-2026-08.md` (BENCH1, vs egui), whose "Next"
list asked for exactly this. Read BENCH1's methodology first; this document
restates only what differs.

Two axes, because they answer different questions and only one of them can
include a non-Rust framework:

* **Frame cost** — in-process, matched stopping points. Rust only.
* **Whole app** — distributable size and idle memory for a "hello" window.
  Every framework, including Flutter.

---

## 1. Frame cost — Lumen vs iced

Same harness shape as BENCH1: an N-row text list in a 400×800 viewport, one
frame per iteration. `benches-competitive/benches/vs_iced.rs`.

* **Lumen** — `pump()` with `NullRenderer`: build, reconcile, lay out (taffy),
  paint to a display list, rebuild the semantics tree.
* **iced** — rebuild the `Element` tree, `Tree::diff`, `layout()`, `draw()`
  into an `iced_tiny_skia::Renderer`. Stops at that renderer's primitives.

iced is the more informative peer than egui: it is **retained and reactive**
like Lumen, so a gap is an optimisation backlog rather than a category
difference.

### 1a. Steady state — one row's text changes per frame

| rows | Lumen | iced | ratio |
|-----:|------:|-----:|------:|
| 100 | 116.0 µs | 15.7 µs | 7.4× |
| 250 | 242.9 µs | 31.7 µs | 7.7× |
| 500 | 438.6 µs | 58.0 µs | 7.6× |
| 750 | 614.3 µs | 85.2 µs | 7.2× |
| 1000 | 820.1 µs | 112.3 µs | 7.3× |
| 1400 | 1127.7 µs | 154.8 µs | 7.3× |
| 2000 | 1784.2 µs | 219.1 µs | 8.1× |
| 3000 | 2736.2 µs | 327.9 µs | 8.3× |

**Flat at 7.2–8.3× across a 30× range.** As in BENCH1, a constant ratio is the
informative result: the two scale the same way and what separates them is a
constant factor, not an algorithmic difference.

### 1b. Cache denied — EVERY row's text changes per frame

This is the group that explains the one above, and it is the reason this
document exists rather than a one-line ratio.

| rows | Lumen | iced | ratio |
|-----:|------:|-----:|------:|
| 100 | 1.673 ms | 0.583 ms | 2.9× |
| 500 | 3.572 ms | 2.951 ms | 1.2× |
| 1000 | 6.063 ms | 5.918 ms | 1.02× |
| 3000 | 16.658 ms | 17.759 ms | **0.94×** |

**With both text caches denied, the frameworks converge — and at 3000 rows
Lumen is 6% FASTER than iced.**

Put the two tables together and the gap localises exactly:

| at 3000 rows | churn (no cache) | steady state | speedup the cache buys |
|---|---:|---:|---:|
| iced | 17.759 ms | 0.328 ms | **54×** |
| Lumen | 16.658 ms | 2.736 ms | **6.1×** |

Lumen's *pipeline* — build, reconcile, layout, paint, semantics — is already
competitive with iced's on raw throughput. What is 7–8× behind is the
effectiveness of its **text caching in the steady state an app actually lives
in**. iced's per-widget paragraph cache turns 2999 unchanged rows into almost
no work; Lumen's recovers only 6×.

This is the same conclusion BENCH1 reached against egui — "it was one
framework's string cache working and the other's defeating itself" — now
confirmed against a second, architecturally closer framework, and quantified
rather than inferred.

### The trap this harness had to avoid

`iced_core` ships a null renderer, `impl Renderer for ()`, and it is the
obvious thing to reach for. It sets `type Paragraph = ()`, so **text shaping
becomes a no-op**. Benchmarking a text list against it would have compared
Lumen's parley/swash shaping against iced doing nothing and produced a
flattering, meaningless number — the same failure BENCH1 made once in the
other direction. `iced_tiny_skia` is used instead because its `text::Renderer`
has a real cosmic-text paragraph. Two sanity tests in
`benches-competitive/src/lib.rs` assert iced actually shapes text and lays out
every row, so this cannot silently regress.

---

## 2. Whole app — a "hello" window

### 2a. Distributable size

Every Rust binary built with the **same** profile — `opt-level = "z"`,
`lto = true`, `strip = true`, `codegen-units = 1` — matching
`scripts/size_gate.sh`.

| framework | size | what that is |
|---|---:|---|
| GTK3 (C) | **14 KB** + 30.8 MB | binary is tiny; needs 67 shared libraries totalling 30.8 MB installed |
| iced (wgpu) | **8.4 MB** | single static binary |
| Xilem (wgpu/vello) | **9.7 MB** | single static binary |
| Lumen (softbuffer) | **10.6 MB** | single static binary, no GPU stack |
| Lumen (wgpu) | **13.5 MB** | single static binary |
| Flutter | **22 MB** | bundle: exe + libflutter_linux_gtk.so + libapp.so + icudtl.dat |

Lumen is the largest of the Rust three. Against iced, +5.1 MB for the same
windowed GPU app; the extra is Lumen's own surface — semantics/agent tree,
`.lss` styling, the embedded font subset — not overhead in a shared component.
GTK's 14 KB is real but so is the 30.8 MB it dynamically links; the honest
comparison is 30.8 MB *shared across every GTK app on the machine* against
8–13 MB *per app* for the static ones.

### 2b. Idle memory after first frame

RSS of the process tree, median of 3 runs, measured externally from `/proc`.
Grouped by rendering stack, because a GPU driver dominates the number and
comparing across that line is meaningless.

| framework | idle RSS |
|---|---:|
| **Lumen (softbuffer, no GPU)** | **11.6 MB** |
| GTK3 (cairo/X11) | 29.5 MB |
| | |
| **Lumen (wgpu)** | **157.8 MB** |
| iced (wgpu) | 191.9 MB |
| Flutter (own engine) | 203.9 MB |
| Xilem (wgpu + vello) | 320.2 MB |

**This is Lumen's strongest result.** Among GPU frameworks it is the lightest
measured — 18% under iced, 23% under Flutter, and less than half Xilem. And
its no-GPU profile at 11.6 MB is **2.5× lighter than a minimal C GTK3 app**,
which is the more striking number given GTK is the one framework here that is
not shipping its own renderer.

The wgpu figures are dominated by the NVIDIA driver's mappings, not by
framework allocations — which is exactly why the softbuffer/GTK pair is the
more meaningful comparison for what the framework itself costs.

---

## Honest caveats

**Against Lumen, on frame cost:** `pump()` rebuilds the **semantics tree**
every frame — the accessibility and agent surface. iced has no equivalent, so
Lumen pays for a feature iced does not offer. In the churn group that cost is
amortised into a much larger total, which is part of why the two converge
there; it is not the whole story, since the convergence is to parity and
beyond.

**Against iced:** `Tree::diff` runs every iteration, as a live iced app does,
but the tree is structurally identical frame to frame, so diffing is in its
cheapest case. Lumen's reconciliation is likewise in its cheap case. Symmetric.

**Neither culls.** Both lay out all N rows in a 400×800 viewport.

**Size and memory are one app each, on one machine.** A "hello" window is the
fixed-cost end of the curve and says nothing about how either framework scales
with app complexity.

**The GPU-stack RSS numbers are driver-dominated** and would look different on
another GPU or with lavapipe.

**A contaminated run was discarded.** The first full benchmark ran while three
other builds and a series of GPU window launches were in flight; it reported
iced at 160.8 µs / 251.9 µs for 1000 / 1400 rows against 112.3 / 154.8 on an
idle machine, and showed a 2.2 ms Lumen "cliff" at 1400 rows that did not
reproduce. Only the idle-machine run is reported above. Criterion's own
confidence intervals were tight in both runs and would not have revealed this.

---

## What is NOT measured, and why

* **GTK4** — blocked. Needs `sudo apt install libgtk-4-dev`, which this session
  could not run. GTK3 is measured instead, and it is what Lumen itself links
  through rfd/muda/tray-icon.
* **Xilem frame cost** — only the whole-app axis. Its widget layer (masonry)
  has a `TestHarness` that would support a matched-stopping-point comparison;
  that is the natural next extension.
* **Flutter frame cost** — not comparable in this harness at all. It is Dart
  with its own engine and scheduler; there is no in-process Rust equivalent of
  "build + layout one frame". Its own DevTools timeline would be the
  instrument, and it would measure a different thing.
* **Slint** — BENCH1's "Next" also asked for it; still not done.

---

## What this says about the grade

The bar is relative: A = match the leader, A+ = surpass it.

On **frame cost in the steady state**, Lumen is 7–8× behind iced. Not A.

On **raw pipeline throughput** with caches denied, it is at parity and slightly
ahead at 3000 rows. That is an A-grade result on the axis that measures the
architecture rather than the cache.

On **idle memory**, it is the lightest GPU framework measured and 2.5× lighter
than GTK3 without a GPU. That is the first axis where the evidence supports A+.

On **binary size**, it is the largest of the Rust three.

The actionable conclusion is narrow and well-supported: **the gap is text
caching in the steady state, not the pipeline.** BENCH1 pointed at the same
thing from a different angle; BENCH2 measures it directly and puts a number on
it — a 54× cache against a 6× one.

## Reproducing

```sh
# frame cost (idle machine, nothing else running)
cd benches-competitive && cargo bench --bench vs_iced

# whole-app apps are generated outside the repo; see this file's git history
# for the exact manifests. Size: matched profile above. Memory: median of 3
# samples of summed /proc/<pid>/status VmRSS, 4 s after launch on DISPLAY=:0.
```
