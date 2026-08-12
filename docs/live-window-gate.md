# The live-window gate

*2026-08-13. `just live-gate`, `scripts/live_window_gate.{sh,py}`.*

## Why it exists

Every other gate in this repo is headless on the CPU renderer, which has **no
swapchain, no texture-dimension concept and no OS event loop**. Of the five
defects found in the week to 2026-08-12, the three that reached a user were all
in the live surface path, and **none of the 394 headless suites saw any of
them**:

| defect | found by | this gate's leg |
|---|---|---|
| oversize shadow sprite panics `create_texture` | Mercurium, live window | `oversize` |
| window over 2048 px aborts at open | inspection | `oversize` |
| resize storm panics `Surface::configure` | Mercurium, live window | `resize-storm` |
| `DrawCmd::Image.src_rect` dropped on GPU — every shadow painted wrong | reading code | `shadow-ink` |
| wheel inverted in secondary windows | reading code | `multi-window` (partial) |

Two independent reasons they were invisible: `DefaultRenderer = TinySkia`, so
tests never touch a swapchain; and CI's GPU job (now blocking) still runs
`cargo test`, which never opens a window.

## What it does

Boots examples through `just run-agent`, so input arrives over the **real winit
event path**, then asserts through the agent RPC — which can see far more than
liveness: `ui.getTree`, `ui.getLayout`, `ui.probeRegion` (pixels),
`app.diagnostics`, `ui.getWindows`.

| leg | example | asserts |
|---|---|---|
| `boot` | counter | tree answers; the log says `present = direct-to-surface` |
| `input` | counter | `input.click` changes `#value`; a drag off the button changes nothing |
| `shadow-ink` | counter | the strip under `#card` differs from the page background |
| `diagnostics` | counter | a stock example reports no diagnostics |
| `multi-window` | counter | `ui.getWindows` answers |
| `oversize` | counter | a 2600×1500 window neither aborts nor goes unresponsive |
| `resize-storm` | datagrid | 400 seeded random resizes; alive **and still on the direct path** |

It is a **smoke** gate: liveness, crash-freedom, gross correctness. Pixel parity
belongs to `cpu_vs_gpu` and stays there.

## Design decisions that matter if you edit it

* **Liveness is polled by pid, not by the socket.** The failure this gate exists
  for is a panic, and a panicked process stops answering RPC in exactly the way
  a busy one does. Asserting on RPC timeouts would turn a crash into a flake.
* **A missing display or adapter is a FAILURE, not a skip.** Same rule the `gpu`
  job states in its own comment: a gate that self-skips reports green while
  proving nothing.
* **`resize-storm` also fails on a CPU-readback fallback.** Surviving by
  degrading is the *other half* of the SR1 bug (a transient skip misread as a
  dead surface), so a run that quietly stops presenting directly must not pass.
* **The storm is seeded** (`random.Random(0x11FE)`). A gate that fails one run
  in ten and passes the next teaches people to re-run it.
* **The example binary is built before any timer starts.** The first version
  raced a cold release build against the "did a window appear" wait and reported
  a window failure for a build that had not finished.
* **New pids are diffed against a pre-launch snapshot.** A leftover window from
  an earlier run was picked up as the subject on the first run.
* **`shadow-ink` reads the RAW tree** (`ui.getTree {"raw": true}`). `#card` is an
  unlabelled `Group`, so elision folds it away and every selector-based verb
  resolves through the elided view and cannot see it — the node carrying the
  shadow is exactly the kind a11y drops.

## Proven against the real bugs

Both directions were checked, not assumed:

* **`resize-storm`**: with `e346f46` (the SR1 fix) reverted and the binary
  rebuilt, the leg fails with the original `Surface::configure: Invalid surface`
  panic. With the fix in, 400 resizes pass and the direct path survives.
* **`shadow-ink`**: with `card.shadow` set to `None`, the leg fails with "the
  strip below #card matches the page background". With the shadow, it passes.

A leg that stops corresponding to a real failure mode should be **deleted**,
not kept for the count.

## Running it

```
just live-gate                          # everything, 400-resize storm
just live-gate --legs boot,input        # a subset
just live-gate --storm 40               # shorter storm
```

Needs an X display and `wmctrl`. CI runs it under `Xvfb` **plus openbox** — an
EWMH resize request needs a window manager to honour it, so xvfb alone is not
enough. The CI storm is 120 rather than 400: software rasterization is slow and
the storm's job is to race a reconfigure against a resize, which 120 does.

## What it does not cover

* **Wheel direction in a secondary window** — the actual inversion bug. The
  `multi-window` leg only checks `ui.getWindows` answers, because no example
  opens a second window today. Closing this properly needs an example that does;
  until then that bug's regression cover is `input.scroll` on the primary only.
* **Real GPU hardware.** CI runs lavapipe. The lavapipe gradient defect
  (`docs/gl-backend-gradient-defect.md`) is why no pixel-parity assertion lives
  here.
