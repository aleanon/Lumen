# Lumen vs. the Linux Mint Update Manager (GTK 3 / PyGObject)

*Measured 2026-08-05 on the dev box: Linux Mint 22.3, i9-13900KF, RTX 4070
(NVIDIA 595.84), X11. `mintupdate 7.1.4`. Lumen at `2aeef92`, release builds.*

Motivating question: mintupdate feels fast and light. What is it actually doing,
how much of that is real, and what of it can Lumen adopt?

**Short answer.** The perception is correct, and the reason is three specific
architectural choices — not a vague "GTK is lighter". Two of them are directly
actionable for Lumen and are already what the CP-series targets. The third is a
deliberate trade Lumen makes, but it is currently unavoidable even when the trade
isn't wanted, which is a bug-shaped gap.

*Follow-up: §4 (idle CPU) and §7(c) (the GPU context) were investigated
separately — see `docs/results-idle-and-gpu-context.md`. §4's original analysis
was wrong and is corrected in place.*

---

## 1. What mintupdate actually is

- **Python 3 + GTK 3 via PyGObject.** `mintUpdate.py` is ~2 400 lines;
  `gi.require_version('Gtk','3.0')`, plus `Gdk`, `Gio`, `GLib`, `GObject`,
  `Notify`, `Pango`.
- **Static UI from Glade XML** via `Gtk.Builder` — parsed once at startup into a
  retained widget tree that is then never rebuilt.
- **The update list is a `GtkTreeView` over a 12-column `Gtk.TreeStore`**, with a
  handful of `CellRendererText`/`Toggle`/`Pixbuf` objects.

That last point is the crux and deserves stating precisely: **a `GtkTreeView`
creates no widgets per row.** The model holds data; a few *stateless* cell
renderers are reused as stamps to draw each visible row. 500 rows is 500 rows of
data and roughly zero per-row objects, and only the rows inside the viewport are
ever touched.

---

## 2. The decisive measurement

Same workload both sides — a 500-row list where one row's text changes. GTK
driven synchronously (`while Gtk.events_pending(): Gtk.main_iteration()`) so the
redraw is included; Lumen from `benches/benches/nodecost.rs`.

| 500-row list | GTK 3 TreeView (Python) | Lumen |
|---|---|---|
| change **one** row | **31.1 µs** | **1 114 µs** |
| rebuild **all** rows | 10 190 µs | **776 µs** |
| **incremental ÷ full** | **0.003** | **1.44** |

Two independent facts here, and they point opposite ways:

- **Lumen's full rebuild is 13× faster than GTK's.** Rust against Python +
  GObject marshalling over 500 `append` calls. Raw throughput is not the problem.
- **GTK's incremental path is 36× faster than Lumen's** — and, more damningly,
  **328× cheaper than its own full rebuild**, where Lumen's "incremental" path is
  **1.44× more expensive** than its own full rebuild.

The ratio column is the honest headline, because it is internally consistent
within each toolkit and immune to the language difference. GTK's incremental
machinery pays off by ~330×. Lumen's currently costs more than not having it.

*Caveat, stated so nobody over-reads the 36×:* Lumen's figure is a headless pump
that includes a full CPU raster (tiny-skia AA is not translation-invariant, so
`render_damage` re-renders and crops — decision log R2), while GTK's includes a
cairo draw of only the dirty row. The absolute gap is therefore somewhat
overstated. The **ratio** is not.

---

## 3. Memory

| | mintupdate | Lumen `counter-win` | Lumen `datagrid-win` |
|---|---|---|---|
| RSS | **89 MB** | **292 MB** | 270 MB |
| PSS (proportional) | 51 MB | 218 MB | — |
| `Pss_Anon` (genuinely its own) | **43.6 MB** | **69 MB** | — |
| heap | 20 MB | 30 MB | — |
| threads | **9** | **46** | 41 |
| `.so` mappings | 850 | 640 | — |

Note `counter-win` — a *trivial counter* — uses **more** than the 1 041-node
datagrid. Lumen's memory is essentially a fixed baseline, not a function of app
content.

**Where Lumen's 292 MB goes** (top mappings):

```
37.9 + 6.7 MB   libLLVM.so.20.1            ← Mesa shader compiler
32.8 MB         libnvidia-gpucomp.so       ← NVIDIA shader compiler
32.0 MB         /dev/nvidiactl  (4 × 8 MB) ← driver mappings
 7.4 MB         libnvidia-rtcore.so
 6.2 MB         libnvidia-glvkspirv.so
─────────────
~123 MB         GPU / shader-compiler residency
29.8 MB         [heap]                     ← the app itself
10.5 MB         the binary (of 34.5 MB on disk)
```

**Where mintupdate's 89 MB goes:** `[heap]` 20 MB (mostly the APT package cache —
app data, not toolkit), `libgtk-3` 5.4 MB, `librsvg` 2.8 MB. **No LLVM, no GPU
driver, no Vulkan.**

Toolkit floors, measured directly:

| | RSS |
|---|---|
| `python3` + `import gi` + GTK 3 | **37 MB** |
| …+ window + 200-row TreeView | **49 MB** |
| Lumen windowed baseline | **~285 MB** |

So on the fairest metric — memory the process genuinely owns — it is 44 MB vs
69 MB, which is a normal 1.6× and not scandalous. On RSS, the number a user sees
in a task manager, it is 89 MB vs 292 MB, and **~123 MB of that difference is
GPU driver and shader-compiler residency that a cairo-on-CPU app never pays.**

---

## 4. Idle CPU

Window mapped and visible, sampled over 10 s:

| | idle CPU |
|---|---|
| mintupdate | **0.00 %** (literally 0 jiffies) |
| Lumen `counter-win` | 0.40 % |
| Lumen `datagrid-win` | **1.90 %** |

> **⚠️ Corrected 2026-08-06 — the analysis that was here was wrong on both counts.**
> It claimed the idle CPU was "on the main thread" and "a bug, not a design cost".
> Investigation (`docs/results-idle-and-gpu-context.md`) found:
> - **The main thread is at zero.** `about_to_wait` is called **once** in 12 s and
>   enters `ControlFlow::Wait`. Lumen's event loop is correct.
> - **It is the NVIDIA driver**, running a 100 Hz `FUTEX_WAKE` + 10 ms timed-wait
>   loop in two of its own threads. Same binary on lavapipe: **0 jiffies over
>   20 s**. Not Lumen's bug and not fixable in Lumen.
>
> The original numbers came from a 10 s sample taken 3 s after launch — startup
> tail, at a resolution where one stray wakeup dominates. Re-measured over 30 s
> after settling, per-thread, the main thread is flat zero.

Lumen's shell has the right structure — `ControlFlow::Wait` when idle,
`WaitUntil` for scheduled wakes, `Poll` only for continuous animation
(`lumen-shell/src/lib.rs:851`) — and it reaches it.

The residual is the price of **holding a Vulkan context**, which is
driver-dependent and outside Lumen's control. The only lever Lumen has is not
creating one — see §7(c), which the same investigation showed is currently
impossible.

---

## 5. Startup (time to mapped window, warm cache, 3 runs)

| | runs | median |
|---|---|---|
| mintupdate *(includes loading the APT cache)* | 404 / 309 / 303 ms | **309 ms** |
| Lumen `counter-win` | 332 / 250 / 175 ms | **250 ms** |

**Lumen starts faster**, despite GPU context creation, and mintupdate's figure is
flattered by nothing — it includes real app work (APT cache) that Lumen's counter
doesn't do. Startup is not a Lumen weakness.

---

## 6. Architectural comparison

| | GTK 3 / mintupdate | Lumen |
|---|---|---|
| widget tree | **retained**, built once from Glade XML, never rebuilt | rebuilt (or copy-forwarded) per changed frame |
| list of N rows | model + ~0 per-row objects; **only visible rows drawn** | N Elements → N tree nodes → N taffy nodes → N entries in ~11 side tables |
| invalidation | **explicit** — `row_changed` → dirty rect → cairo repaints that strip | automatic dependency tracking → scope re-run → rebuild + relayout + re-emit |
| layout | 2-pass size negotiation, cached, reruns only on `queue_resize` | taffy flexbox, whole-tree solve every rebuild |
| rendering | cairo on CPU → X11 surface; no in-process GPU context | wgpu → Vulkan → NVIDIA; glyph atlas, damage-driven present |
| reactivity | none — signals and callbacks you wire by hand | fine-grained signals, F1 scope memoization |

The trade is legible. GTK buys its speed with **manual** invalidation and a
model/view split that forces virtualization on the author. Lumen buys ergonomics
and correctness (automatic dependency tracking, agent introspection,
snapshot/restore, one tree for four consumers) and currently pays for them.

---

## 7. What Lumen should take from this

**(a) The incremental path — already the CP-series' target.** GTK's
incremental/full ratio is 0.003; Lumen's is 1.44. CP1 + CP2 aim at < 0.5. This
comparison says the ceiling is far lower than that, and gives a concrete target
worth aiming past: **CP2's acceptance could reasonably be tightened once CP1
lands.**

**(b) Virtualization should be structural, not opt-in.** GTK cannot accidentally
render 500 rows — `GtkTreeView` only walks the viewport. Lumen has
`widgets::virtual_list` and `PortalList`-style paths, but the *default* is that
500 rows means 500 of everything. `vlist_1m_scroll` at 1.15 ms shows the
virtualized path works; the issue is that it is a widget the author must choose.

**(c) The GPU context should be optional, and today it is worse than optional.**
`LUMEN_RENDERER=cpu` correctly selects `TinySkia` for drawing — but the process
**still maps 225 NVIDIA/LLVM regions and sits at 216–225 MB**. Investigated
(`docs/results-idle-and-gpu-context.md`): choosing the CPU renderer makes
`attach_surface` report it cannot present, which *causes* the shell to build a
**second** wgpu instance/adapter/device purely to blit
(`lumen-shell/src/lib.rs:491`, `:1457`). The branch is inverted from the user's
intent, and there is no CPU-only present path today — that needs `softbuffer`
or a hand-rolled per-platform blit, i.e. an ADR-003 escalation. For an update-manager-shaped app — static list, no animation,
wants to sip battery — that is ~123 MB and a slower start for nothing. A genuine
no-GPU path would put Lumen within striking distance of GTK's footprint.

**(d) Idle CPU — investigated, and it is not ours.** §4. The residual is the
NVIDIA driver's 100 Hz polling, not Lumen's loop
(`docs/results-idle-and-gpu-context.md`). The only lever is (c): don't hold a
GPU context you aren't using. Separately, that investigation found
`ThreadPoolSpawner::default()` spawns `available_parallelism()` threads
unconditionally — 32 here for a counter app that never runs a task.

## 8. What this comparison does *not* say

Lumen is not competing with GTK 3 on GTK 3's terms. GTK 3 does not do
GPU-composited animation, does not run on mobile or web, has no agent
introspection, no snapshot/restore, no `.lss` hot reload, and is not
memory-safe. mintupdate is a static list of packages that changes a few times a
day — close to the best possible case for a retained CPU-drawn toolkit and close
to the worst possible case for justifying a GPU context.

The useful conclusion is narrower and more actionable: **on the axes where
mintupdate wins, it wins for reasons Lumen can adopt without giving anything up**
— a genuinely cheap incremental path, structural virtualization, an optional GPU,
and a UI that sleeps.

## Caveats

- One machine, one GPU vendor. NVIDIA's driver residency is not Mesa's; an Intel
  or AMD box would show a different (likely smaller) GPU tax.
- mintupdate's heap is dominated by the APT cache — that is app data, and it is
  why the 89 MB figure is *not* a GTK toolkit measurement. The toolkit floor
  measurements in §3 are the fair ones.
- GTK's 31.1 µs is measured through a synchronously-pumped main loop; it includes
  the expose and cairo draw but not compositor presentation.

## Reproducing

```
# GTK reference scripts (committed alongside, so the numbers are re-checkable)
DISPLAY=:0 python3 benches/gtkfloor.py    # toolkit floor RSS
DISPLAY=:0 python3 benches/gtkrow.py      # 1-row vs full-rebuild timing
# Lumen side
cargo build --release -p counter@0.0.0 --example counter-win
cargo bench -p lumen-benches --bench nodecost
```
