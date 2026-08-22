# BENCH4 — Qt added, and Lumen re-measured after F2/F3 (2026-08-22)

Supersedes BENCH3 (`results-competitive-bench3.md`). Two things changed since:
Lumen's memoized frame is **half** what it was, and **Qt6** joins the
comparison. Flutter is dropped — its SDK is no longer on this box, and quoting
a number I cannot reproduce is worse than an empty row.

**Two of BENCH3's claims are corrected below.** Read those first if you have
read BENCH3.

Every harness is now *in this repo* (`benches-competitive/harnesses/`). BENCH3
generated its GTK/Slint/Flutter apps outside the tree and they were gone within
three days, which is why this round had to rebuild them from scratch.

---

## Corrections to BENCH3

**1. "Lumen, iced and masonry all lay out and paint every one of the N rows."
That is false for iced and for Lumen — both cull.** iced's `Column::draw`
filters children by `layout.bounds().intersects(viewport)`
(`iced_widget-0.14.2/src/column.rs:316`); Lumen skips display-list emission for
any node fully outside the canvas (`app.rs`, the `offscreen` guard). BENCH3
used this claim to argue that GTK's paint could not be compared because "the
same benchmark asks GTK for ~50 rows of paint work and the others for 3000".
The asymmetry it described does not exist: all three lay out every row and
paint only the visible ones. GTK's paint is still not measured here, but for
the other reason BENCH3 gave — it needs a live frame-clock snapshot pass, and a
synchronous `gtk_widget_snapshot_child` returns NULL.

**2. Lumen is not "static, 13.5 MB".** A default Lumen app links the **GTK3
cluster** — 66 shared objects totalling 28.0 MB, including `libgtk-3`,
`libgdk-3`, atk, at-spi and pango — because `lumen-shell`'s
`desktop-integration` feature pulls `rfd` + `muda` + `tray-icon` (ADR-P1).
BENCH3 listed Lumen beside iced and Xilem as statically linked. Turning the
feature off (`--no-default-features --features wgpu`) makes it genuinely
static — 0.2 MB in 1 shared library — and costs **0.3 MB of executable**.

---

## 1. Frame cost — one method for everything

BENCH3 mixed criterion means (Rust) with hand-rolled best-of-N (GTK). Every row
below is the **minimum of 200 iterations** after warm-up, `taskset -c 2`, same
box, same 3000-row list with one row's text changing per frame. The minimum is
the least-interfered sample, which matters here: a background process at 100%
CPU twice during this work moved a number by 12–38%.

| framework | µs | what is included |
|---|---:|---|
| **Lumen — patch path** | **93** | binding update + display list (culled) |
| GTK4 — layout only | 112 | `gtk_widget_measure`; no rebuild, no paint |
| Qt6 — forced relayout | 201 | `setText` + `invalidate` + `activate`; no paint |
| iced | 247 | `diff` + `layout` + `draw` |
| Qt6 — natural update | 320 | `setText` + event-loop turn; no paint |
| egui | 779 | full immediate-mode rebuild + `tessellate` |
| **Lumen — rebuild path** | **840** | build + reconcile + layout + display list |
| masonry (Xilem) | 877 | `edit_widget` + layout + vello `Scene` |

**These are not all the same measurement, and the table is ordered by cost, not
by merit.** GTK's row is layout alone. Qt's rows exclude rasterisation, which
Qt otherwise does and the Rust harnesses never do — Qt's own floor for
rendering the 400×800 viewport is **260 µs**, more than Lumen's entire patch
frame. A row-to-row ratio is only meaningful where the "included" column
matches.

The two comparisons that *are* like-for-like:

* **Lumen vs iced vs masonry vs egui** — all four rebuild a view, lay out every
  row, and stop at their display-list analogue with no rasterisation.
* **Lumen's two paths against each other** — same framework, same content.

### Scaling

| rows | Lumen patch | Lumen rebuild | iced | masonry | egui |
|-----:|------------:|--------------:|-----:|--------:|-----:|
| 100 | 41.4 | 65.9 | 13.9 | 32.1 | 37.5 |
| 500 | 51.0 | 174.3 | 45.5 | 133.8 | 136.6 |
| 1000 | 60.2 | 306.5 | 94.4 | 275.5 | 260.9 |
| 3000 | 92.9 | 839.7 | 248.6 | 886.6 | 778.8 |

**The shape is the result, not the 3000-row row.** Lumen's patch path grows
2.2× across a 30× increase in rows; everything else is linear (iced 17.9×,
egui 20.8×, masonry 27.6×). At 100 rows iced is the fastest thing here and
Lumen's patch is 3× slower than it. Interpolating between the 500- and
1000-row samples, the two cross at **~570 rows**; by 3000 Lumen's patch is
2.7× faster than iced.

That crossover is the honest summary of where Lumen now stands: a higher fixed
cost per frame, and a much flatter curve, because a patch re-emits only the
culled display list while everyone else rebuilds a view proportional to N.

### Churn — cache denied

Criterion, 3000 rows, every row's text changed every frame (no memo can help):
**Lumen 15.27 ms, iced 17.89 ms.** Lumen is faster when nothing can be reused,
which is the case its text and style caches are least able to help with.

---

## 2. Whole app

Same app everywhere: a window with a label and a button, shown, then idle.

### Distributable size

`ldd`-resolved unique shared objects, excluding the C runtime every binary
already has. A statically linked Rust binary therefore reports its own size and
almost nothing else.

| framework | executable | dynamic | libs |
|---|---:|---:|---:|
| GTK3 (C) | 18 KB | 27.9 MB | 65 |
| GTK4 (C) | 17 KB | 32.1 MB | 65 |
| Qt6 (C++) | 18 KB | 70.1 MB | 45 |
| iced | 10.7 MB | 0.2 MB | 1 |
| Xilem | 10.8 MB | 0.2 MB | 1 |
| Slint | 14.8 MB | 2.0 MB | 9 |
| **Lumen — lean** | **15.9 MB** | **0.2 MB** | **1** |
| **Lumen — default** | **16.2 MB** | **28.0 MB** | **66** |

Lumen's default build is the heaviest total here. The executable is comparable
to Slint's and ~50% above iced's; the 28 MB is GTK3, and it is optional.

### Idle RSS after first frame

Median of 3, `VmRSS` 4 s after launch, `DISPLAY=:0`.

| framework | stack | idle RSS |
|---|---|---:|
| GTK3 | cairo / X11 | 29.6 MB |
| **Qt6** | raster / XCB | **46.2 MB** |
| Slint | femtovg / GL | 68.7 MB |
| GTK4 | GSK / GPU | 136.3 MB |
| Lumen — lean | wgpu | 162.1 MB |
| Lumen — default | wgpu | 168.6 MB |
| iced | wgpu | 195.8 MB |
| Xilem | wgpu + vello | 320.7 MB |

The method reproduces BENCH3 closely — GTK3 lands on 29.6 MB again, Slint
within 3%, GTK4 within 2% — which is the reason to trust the new Qt row.

**Qt is the lightest GPU-era toolkit here by a wide margin**, because its
default raster paint engine is not a GPU stack at all. The defensible Lumen
claim remains the narrow one BENCH3 arrived at: **lightest of the wgpu-based
frameworks**, now 162–169 MB against iced's 196 MB and Xilem's 321 MB.

---

## Caveats

* **Qt's frame numbers exclude rasterisation and the others exclude it too, but
  Qt is the only one that would otherwise do it.** Its 260 µs floor is real
  work; subtracting it is not obviously right either.
* **Qt relayouts the whole box layout for one changed label** (201 µs at 3000
  rows). Retained-mode does not mean incremental layout.
* **GTK has no paint row.** Not an oversight — see the corrections above.
* **Slint has no frame row**, unchanged from BENCH3: its testing backend does
  not paint, so there is no comparable instrument.
* **No Flutter row.** SDK gone; not re-measured; not quoted.
* Every number is Linux/X11 on one i9-13900KF with an RTX 4070. RSS on a
  GPU stack is a floor, not a total — driver-side allocations are not in
  `VmRSS`.

## Reproducing

```sh
cd benches-competitive
cargo build --release --bin probe_frame && taskset -c 2 ./target/release/probe_frame 3000 200
cmake -S harnesses/qt -B /tmp/qtb && cmake --build /tmp/qtb && QT_QPA_PLATFORM=offscreen taskset -c 2 /tmp/qtb/qt_frame 3000 200
gcc -O2 -o /tmp/g4 harnesses/gtk/frame.c $(pkg-config --cflags --libs gtk4) && taskset -c 2 /tmp/g4 3000 200
cd harnesses/apps && cargo build --release        # the four Rust apps
harnesses/measure.sh size <exe>; harnesses/measure.sh rss <exe>
```
