# Investigation: idle CPU and the unavoidable GPU context

*Measured 2026-08-05/06. Linux Mint 22.3, i9-13900KF, RTX 4070 (NVIDIA 595.84),
X11. Lumen at `e880316`, release builds. Follow-up to
`docs/comparison-gtk-mintupdate.md`, which raised both issues — and got the
first one wrong.*

---

## Summary

| | verdict |
|---|---|
| **Idle CPU** | **Not Lumen's bug.** Lumen's event loop sleeps correctly. The CPU is a 100 Hz polling loop inside the **NVIDIA proprietary driver**. Same binary on lavapipe: **0 jiffies**. |
| **GPU context** | **Lumen's design, and backwards.** Selecting the CPU renderer doesn't avoid wgpu — it *forces a second* wgpu device, because CPU-rasterized pixels have no non-GPU path to the window. |

The comparison document claimed the idle CPU was "a bug, not a design cost" and
"on the main thread". **Both halves were wrong**, and §1.4 below records why the
first measurement misled. Corrected there and in that document.

---

## 1. Idle CPU

### 1.1 Lumen's event loop is correct

`Shell::about_to_wait` (`lumen-shell/src/lib.rs:819`) picks `Poll` /
`WaitUntil` / `Wait` from `Headless::next_deadline()`. Instrumented with a
temporary env-gated probe and run for 12 s on an idle `counter-win`:

```
[idle] about_to_wait#0 branch=Wait continuous=false next_deadline=None min_dt=None
```

**One call, in 12 seconds.** The loop entered `ControlFlow::Wait` and blocked.
`requests.continuous` was false and no wake was scheduled. There is nothing to
fix here.

### 1.2 The CPU is in two driver threads doing a 100 Hz timed wait

Per-thread sampling over 30 s (main thread = pid):

```
tid=2234215  jiffies=7
tid=2234231  jiffies=6
main thread: 0
```

`strace -f -tt` on the steady state shows exactly what those threads do:

```
futex(0x565ff61fc4b0, FUTEX_WAKE_PRIVATE, 1) = 0
futex(0x7875a0000d48, FUTEX_WAIT_BITSET_PRIVATE|FUTEX_CLOCK_REALTIME, 0,
      {tv_sec=…, tv_nsec=731808000}, FUTEX_BITSET_MATCH_ANY) = -1 ETIMEDOUT
futex(0x565ff61fc4b0, FUTEX_WAKE_PRIVATE, 1) = 0
futex(0x7875a0000d48, FUTEX_WAIT_BITSET_PRIVATE|FUTEX_CLOCK_REALTIME, 0,
      {tv_sec=…, tv_nsec=742065000}, FUTEX_BITSET_MATCH_ANY) = -1 ETIMEDOUT
```

Deltas: 10.1 ms, 10.2 ms, 10.8 ms — a **100 Hz wake-partner-then-sleep loop**,
two of them, running forever. 14 001 futex calls in a 30 s trace, ~6 900
`ETIMEDOUT`.

### 1.3 It is the NVIDIA driver, proven by swapping the ICD

Identical binary, identical Lumen code, only `VK_DRIVER_FILES` differs:

| Vulkan ICD | threads | RSS | idle CPU (20 s) |
|---|---|---|---|
| `nvidia_icd.json` | 46 | 225 MB | **0.65 %** (13 jiffies) |
| `lvp_icd.json` (lavapipe, software) | 173 | 244 MB | **0.00 %** (**0 jiffies**) |

Zero. Lavapipe spawns 173 threads and still costs nothing at idle, because they
all park.

Corroborating: Lumen's `ThreadPoolSpawner` workers block on
`std::sync::mpsc::Receiver::recv()` with **no timeout** (`tasks.rs:255-268`), so
they cannot be the source, and a repo-wide search finds no 10 ms timed wait
anywhere in `lumen-core`, `lumen-shell` or `lumen-widgets`.

**Conclusion: not actionable in Lumen.** It is the cost of holding a Vulkan
context on this driver. It would not appear on Mesa (Intel/AMD), and mobile
GPU drivers must be measured separately rather than assumed.

### 1.4 Why the first measurement was wrong

The comparison doc sampled **10 s starting ~3 s after launch** and attributed the
jiffies to the main thread. Two errors compounded:

- **Startup tail.** At t=3–13 s the process was still finishing GPU init and
  first paint, so the main thread genuinely had recent CPU. Re-measured at
  t=15–45 s, the main thread is flat zero.
- **A single short sample.** 4 jiffies at HZ=100 is 40 ms; at that resolution
  one stray wakeup dominates. The 30 s windows used here are stable to ±1 jiffy.

Lesson worth keeping: **sample after the app has settled, and prefer per-thread
over per-process** — a process-level number cannot distinguish "our loop is
spinning" from "a driver thread ticks".

### 1.5 What is still worth doing

Not the idle loop, but two things this turned up:

- **`ThreadPoolSpawner::default()` spawns `available_parallelism()` threads
  unconditionally** — **32 on this box for a counter app that never runs a task**
  (`tasks.rs:274-282`). They park on a channel and cost no CPU, but they cost
  stacks, scheduler entries and ~900 MB of `VmSize`. A lazily-grown pool, or one
  sized to `min(4, cpus)` until the first `spawn`, would be strictly better —
  and matters more on a phone than here.
- **A `wake_reason` diagnostic.** "Why is my app not idle?" took a custom probe,
  strace and an ICD swap to answer. The F4 machinery already records why a pump
  happened; surfacing `next_deadline` + the last wake cause through `app.perf`
  would make this a one-line answer. This is genuinely useful even though the
  answer this time was "not us".

---

## 2. The unavoidable GPU context

### 2.1 What happens

`LUMEN_RENDERER=cpu` (or `--tiny-skia`) is honoured for *rasterization* —
`renderer_override()` (`lumen-widgets/src/lib.rs:238-252`) correctly returns
`TinySkia`. But the process still maps **225 NVIDIA/LLVM regions** and sits at
**216–225 MB**.

The cause is presentation, not rasterization (`lumen-shell/src/lib.rs:491-497`):

```rust
self.direct = headless.attach_surface(window.clone().into(), …);
self.presenter = if self.direct { None } else { Some(Presenter::new(window.clone())) };
```

and `Presenter::new` (`:1457`) builds a full wgpu stack of its own:

```rust
let instance = wgpu::Instance::default();
let surface  = instance.create_surface(window)…;
let adapter  = block_on(instance.request_adapter(…))…;
let (device, queue) = block_on(adapter.request_device(…))…;
```

So the branch is **inverted from the user's intent**:

| renderer | `direct` | wgpu devices created |
|---|---|---|
| wgpu | true | 1 (the renderer's own) |
| **CPU / TinySkia** | **false** | **1 — created *because* the renderer is CPU** |

Asking for the CPU renderer never avoids the GPU; it guarantees a wgpu context
whose only job is to blit a CPU-rasterized image to the window. The code comment
says so plainly: *"Falls back to a CPU-readback Presenter when the backend can't
present (CPU renderer / unsupported adapter)."*

### 2.2 What it costs

| | RSS | threads |
|---|---|---|
| wgpu renderer (direct present) | 249 MB | 46 |
| CPU renderer (+ blit Presenter) | 216 MB | 46 |
| GPU/shader-compiler residency in both | ~123 MB | — |

The CPU path saves ~33 MB (the renderer's pipelines; the Presenter uses
`MemoryHints::MemoryUsage`) and **none** of the ~123 MB driver + LLVM residency,
which is the part that matters.

### 2.3 Why there is no CPU-only path today

There is no way to get pixels onto a `winit` window without a graphics API.
`winit` deliberately does not do presentation. The options:

1. **`softbuffer`** — the canonical answer, maintained by `rust-windowing`
   alongside winit; presents a CPU buffer via X11 `SHM`/`XPutImage`, Wayland
   `wl_shm`, and equivalents on macOS/Windows. **New runtime dependency ⇒
   ADR-003 escalation.**
2. **Hand-rolled per-platform present** — X11 `XShmPutImage` via `x11rb`,
   Wayland `wl_shm`, `CGImage`, GDI. Same escalation, several times the work,
   and re-implements a maintained crate.
3. **Do nothing** and accept that a Lumen window always holds a GPU context.

### 2.4 Recommendation

This is worth an ADR escalation for `softbuffer`, on three grounds:

- **Mobile is first-class.** Holding a Vulkan context for a static list view is
  the wrong trade on a battery-powered device, and — per §1.3 — the idle cost of
  that context is driver-dependent and outside Lumen's control. Not creating it
  is the only lever Lumen actually has.
- **It closes most of the GTK gap.** ~123 MB of the 292 MB vs 89 MB difference is
  exactly this. A softbuffer path would put a static Lumen app within range of
  GTK's footprint, which §7 of the comparison doc identified as the one axis
  where Lumen is structurally behind.
- **The seam already exists.** `Renderer`/`attach_surface`/`Presenter` is already
  the abstraction; a `SoftPresenter` slots in beside `Presenter` with no change
  to the renderer trait, and `TinySkia` is already the golden reference (ADR-002)
  so correctness is covered by the existing R0 differential.

Scope sketch: a `SoftPresenter` behind a default-off `softbuffer` feature,
selected when `direct == false`, with `Presenter` kept as the fallback where
softbuffer has no backend. Gate on R0 `cpu_vs_gpu` and the golden suite.

**Not proposing it as a phase yet** — it is an ADR-003 escalation, which is the
user's call, and it should be weighed against the CP-series rather than bolted
on.

---

## Reproducing

```bash
# idle CPU, per-thread, after settling (the only reliable way)
DISPLAY=:0 ./target/release/examples/counter-win &
sleep 15; P=$(pgrep -x counter-win)
for t in /proc/$P/task/*; do echo "$(basename $t) $(awk '{print $14+$15}' $t/stat)"; done   # …sleep 30, diff

# attribute it: same binary, different Vulkan driver
DISPLAY=:0 VK_DRIVER_FILES=/usr/share/vulkan/icd.d/nvidia_icd.json ./…/counter-win   # 0.65 %
DISPLAY=:0 VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json    ./…/counter-win   # 0.00 %

# the 100 Hz loop itself
DISPLAY=:0 timeout 25 strace -f -tt -e trace=futex -o /tmp/st.txt ./…/counter-win

# GPU context under the CPU renderer
DISPLAY=:0 LUMEN_RENDERER=cpu ./…/counter-win &
grep -icE 'nvidia|libLLVM' /proc/$(pgrep -x counter-win)/maps    # 225
```
