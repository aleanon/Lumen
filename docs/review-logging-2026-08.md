# Logging review — where the agent is flying blind (2026-08-23)

## The finding in one paragraph

Lumen already has the right *mechanism*: `Runtime::log(level, message)`
(`lumen-core/src/state.rs:507`) appends to a 1000-entry ring that the agent
pages through with `app.logs {since}` (`lumen-agent/src/lib.rs:630`). What it
does not have is *call sites*. The entire framework emits **two** log lines:

| Site | Message |
|---|---|
| `lumen-app/src/app.rs:3309` | `E0701 build panicked: …` |
| `lumen-app/src/app.rs:3950` | `stylesheet rejected (N diagnostics)` |

Everything else an agent would want to know — which renderer it actually got,
why a pump didn't repaint, why a cache started thrashing, why a click landed on
nothing — is either silent, or printed to **stderr**, which an agent talking
JSON-RPC to a running window cannot read.

Two structural gaps follow from that, and they organize the rest of this
document:

1. **The stderr/ring split.** `lumen-shell/src/lib.rs` has ~20 `eprintln!`
   sites carrying exactly the facts an agent needs (renderer name, present
   mode, GPU→CPU degradation, reload results, window failures). None reach
   `app.logs`. Under `just run-agent`, stderr goes to the terminal and the
   agent sees nothing.
2. **Counters exist but aren't surfaced.** `nodes_rebuilt` / `nodes_copied`
   (`app.rs:801`), `style_memo_hits` / `style_memo_misses` (`app.rs:808`) and
   the text-cache epoch state are all maintained every frame and reachable in
   Rust, but `app.perf` returns only `frame_ms_p50/p95`, `frames_rendered`,
   `node_count`. The retained-pipeline work (CP1–CP6, R5) is measured by
   exactly the numbers the agent cannot read.

## Design rules these recommendations follow

Everything below is **edge-triggered**: log the *transition*, not the state.
The codebase is explicit that the hot path must not pay for diagnostics
(`W0107`/`W0109` are "reported once per declaration at parse time" for this
reason), and a per-frame log line would blow the 1000-entry ring in 8 seconds
at 120 fps. Concretely:

- **One-shot at init** for capability facts (renderer, present mode, fonts).
- **On transition** for degradations (GPU→CPU, direct→readback, cache
  regime change) — with a latch so a flapping condition logs once per flip,
  not once per frame.
- **Rate-limited / first-N** for per-frame anomalies (slow frame, atlas
  overflow), never unconditional.
- **Levels:** `error` = the app lost functionality; `warn` = degraded or
  suspicious but running; `info` = a fact worth knowing once.

A note on `record_change` (`app.rs:2074`): it is `#[cfg(feature = "snapshot")]`
and compiles to `let _ = (kind, nodes)` in the lean build. Log calls should
**not** copy that gating — `Runtime::log` is unconditional, and the lean build
is exactly where an agent has fewest other introspection tools.

---

## P0 — Silent degradations the agent cannot detect any other way

These change performance or correctness by an order of magnitude and currently
produce no machine-readable signal at all.

### 1. GPU adapter unavailable → CPU renderer

`lumen-render/src/gpu.rs:586` `WgpuFallbackTinySkia::new()` stores
`main: Wgpu::new()`, and `Wgpu::new()` (`gpu.rs:678`) returns `None` on any
adapter failure. Every `Renderer` method then quietly takes the `None` arm
(`gpu.rs:608`, `:625`, `:639`). An agent measuring 40 ms frames has no way to
learn it is on tiny-skia.

`is_gpu()` exists (`gpu.rs:594`) and nothing calls it outside tests.

- **Log (info, once at construction):** `renderer=wgpu backend=<Vulkan|GL|…> adapter="<name>"`
- **Log (warn, once):** `renderer=tiny-skia (no GPU adapter; PRIMARY and SECONDARY both failed)`
- **Also worth warning on:** the adapter came back on `Backends::SECONDARY`
  (GL). Per `gpu.rs:679-692` and `docs/gl-gradient-defect.md`, the GL path
  **drops every gradient in the frame with no validation error**. An agent
  diffing a golden against a GL-rendered frame would see missing gradients and
  no explanation. This is the single highest-value log line in the codebase.

### 2. `wgpu` requested but not compiled in

`lumen-widgets/src/lib.rs:288` — `eprintln!` only. The user asked for GPU,
got CPU, and the agent sees a stderr line it isn't reading.

### 3. Direct-present → CPU readback, mid-session

`lumen-shell/src/lib.rs:952` handles `Present::Unavailable` by flipping
`self.direct = false` permanently and `eprintln!`-ing. This is a permanent
per-frame readback cost for the rest of the session. Ring-visible, `warn`.

Sibling site: `lumen-shell/src/lib.rs:616` prints the *initial*
`present = direct-to-surface | cpu-readback` choice, and `:131` prints
`renderer = <name>`. Both are `info`-worthy facts an agent should be able to
read at session start rather than infer from timings.

### 4. `Present::Skipped` storms

`lumen-shell/src/lib.rs:940` re-requests a redraw and presents the same list
again. Routine once during a resize drag; a *sustained* run of them means the
window is not updating. Log at `warn` **only past a threshold** (e.g. 8
consecutive), so a resize drag stays quiet.

### 5. Text cache thrashing

`lumen-text/src/lib.rs:379` `sweep()` carries a measured 2.2× frame-time
penalty in its own doc comment (3.8 ms → 8.5 ms; 1183 re-shapes/frame at 2000
rows) and is completely silent. Two distinct events deserve logs:

- **Cap retarget** (`lib.rs:396`): `*cap` grew beyond `SHAPE_CACHE_CAP`. This
  is the "your working set outgrew the cache" signal — `info` on each doubling,
  not each sweep.
- **Hard-cap fallback** (`lib.rs:388`): `map.len() >= hard_cap` triggers the
  drop-half path. This is the thrash regime the doc comment describes, and the
  fix is `VirtualList`. `warn`, latched (log on entry to the regime, not per
  sweep).

Call sites: `lib.rs:771` (shape cache) and `lib.rs:829` (run cache).

### 6. Glyph atlas exhaustion

`lumen-render/src/gpu.rs:2197`:
- `AtlasFull::PagesExhausted` → `self.atlas_overflow.set(true)`, and the
  comment says *"Silent on purpose — reporting it would fire on any busy
  frame."* Correct as written, but the *repeated* case is not busy-frame
  noise: `gpu.rs:1506` / `:1866` clear and repack the whole atlas afterwards,
  so a workload that overflows every frame re-uploads every glyph every frame.
  Log at `warn` on **N consecutive overflowing frames**, which is the
  distinction the current comment is missing.
- `AtlasFull::TooBig` already latches `atlas_too_big` → `W0110`. Good; a
  parallel ring entry costs nothing and survives after the diagnostic clears
  (same rationale as the `E0701` log at `app.rs:3309`).

### 7. Glyph raster cache eviction

`lumen-text/src/lib.rs:1317` — evicts at `GLYPH_CACHE_CAP` (8192) with no
recency policy at all (unlike the shaped/run caches). The `misses` counter at
`lib.rs:119` is described as "for tests/diagnostics" and is not reachable from
the agent. Worth an `info` when the cache first hits its cap.

---

## P1 — Behaviour the agent must currently infer from screenshots

### 8. Why a pump did not repaint

`lumen-app/src/app.rs:1056` `pump()` is a five-way branch — `rebuild` /
`restyle_only` / `patch_text_bindings` / `patch_bg_bindings` / idle — chosen
from `force_rebuild`, `time_driven`, `write_changed`, `structural_current` and
`visual_changed` (`app.rs:1096-1116`). "I changed state and the UI is stale"
is the top entry in the `debugging-lumen` skill, and the branch decision is
invisible.

Do **not** log every pump. Log the *surprising* one:

- **warn:** `write_changed && !needs_rebuild && !patched` — a signal was
  written, the frame was declared idle, and nothing was patched. Include which
  predicate vetoed it. This is precisely the stale-UI bug class.
- The `else` idle arm at `app.rs:1183` already calls `record_change("idle", …)`,
  which is snapshot-gated — so in a lean build even that trace is gone.

### 9. Text-binding patch declined → full rebuild

`app.rs:1175` — `patch_text_bindings()` returned false because the new string
measures differently, so the frame falls back to a full rebuild. That is the
fast-path-missed signal for the F3.5 work. `info`, rate-limited.

### 10. Copy-forward disabled

`app.rs:1160-1165` — `allow_copy_forward = !visual_changed && !full_rebuild_forced()`.
When `LUMEN_FULL_REBUILD=1` is set (the A.3.5 bisect hatch,
`full_rebuild_forced()`), *every* retained-pipeline optimization is off and
frame times are unrepresentative. An agent benchmarking a build with that
env var set would report garbage. Log it **once at startup**, `warn`.

### 11. Forced cache clears

`app.rs:2769` (`clear_view_caches`), `:3959` and `:4002` (`style_memo.clear()`
on stylesheet set / theme switch). Each throws away memoization and makes the
next frame an outlier. `info` is enough — the value is correlating a frame-time
spike with its cause.

### 12. Clicks that resolve but do nothing

`lumen-agent/src/lib.rs:748` `input.click` returns `{"ok": true}` whenever the
selector resolved, regardless of whether the synthesized press hit a handler.
The routing walk in `app.rs:2195-2210` tracks exactly this
(`did_focus`, `did_click`, `did_drag` — all three can end false), and the
result is discarded.

- **warn:** pointer-down bubbled to the root with no handler fired, at (x, y).
- **warn:** the press was consumed by an overlay/scrim rather than the intended
  node — `dismiss_outside` (`app.rs:2441`) is the site.
- **warn:** the target was disabled (`is_disabled`, `app.rs:2574`) — currently
  indistinguishable from a working click in the protocol response.

This closes the "I clicked it and nothing happened, and the tool said ok" loop
without changing the protocol's return shape.

### 13. Keyboard input with no focus owner

`app.rs:2469` `focused_node()` returning `None` while a key event routes means
the keystroke is dropped. `debugging-lumen` lists "keyboard input goes
nowhere" as a symptom; nothing currently records it.

### 14. Selector resolution failures

`lumen-agent/src/lib.rs:1046` `resolve_selector` / `:1081` `resolve_err_msg`
return an error to the caller — fine for a synchronous call, but the *pattern*
(the same selector failing repeatedly across a session, or `W0302` legacy
`node-<index>` handles being accepted) is only visible in aggregate. Ring
entries make that legible.

---

## P2 — Task, state and reload lifecycle

### 15. Deferred results

`lumen-core/src/tasks.rs:275` `drain_deferred()` returns a count that
`app.rs:1068` discards. An async fetch that lands is invisible; an async fetch
that *never* lands is equally invisible. Log at `info` when a drain applies
> 0 ops (the "your data arrived on frame N" line), and consider a `warn` for a
drain that applies ops after the owning scope died.

### 16. Cancelled / superseded tasks

`lumen-app/src/tasks.rs:202` (resource generation superseded) and `:361`
(registering cancels the previous generation). A deps-thrash loop — a
dependency that changes every frame, cancelling and respawning a fetch forever —
is a real and hard-to-see bug. `warn` on N supersessions of the same key within
a short window.

`lumen-app/src/tasks.rs:351` — *"Fall back to an inert slot rather than
panicking"* when the runtime is unavailable. A silently inert task is a
guaranteed-confusing failure. `warn`.

### 17. Task errors

`lumen-app/src/tasks.rs:127` `Err(e) => c.error = Some(e)` — the error is
stored on the resource cell for the view to render. If the view doesn't render
it (common), the failure is invisible. Mirror every `TaskError` into the ring
at `warn`.

### 18. Successful stylesheet reloads

`app.rs:3945` logs the *rejection* but not the acceptance. Under `run` with
hot reload, the agent cannot distinguish "reload applied" from "watcher never
fired". `lumen-shell/src/lib.rs:508` prints `lumen reload: ok` to stderr only.
`info`.

### 19. Snapshot restore drops

`W0002` (dropped unknown state field on restore) is a defined code; the
restore path at `app.rs:417` returns diagnostics that a caller may ignore.
Ring-mirror at `warn`.

### 20. Panics contained by the error boundary

`app.rs:3309` already does this for build panics — good, and it is the model
the rest of this document generalizes. Check whether layout and paint
boundaries have the same coverage (the `E0701` doc says "build/layout/paint").

---

## P3 — Perf counters to surface rather than log

These are better as `app.perf` fields than ring entries, because they are
continuous quantities, not events. Logging them per frame would be wrong;
omitting them entirely is what forces an agent to guess.

Extend `app.perf` (`lumen-agent/src/lib.rs:614`) with:

| Field | Source | Why |
|---|---|---|
| `nodes_rebuilt`, `nodes_copied` | `app.rs:1209` (already in `FrameStats`) | the retained-pipeline ratio; a copy rate near 0 means memoization is off |
| `style_memo_hits`, `style_memo_misses` | `app.rs:1432` (`style_memo_stats`, exists, unexposed) | A.5b restyle effectiveness |
| `shape_cache_len`, `shape_cache_cap` | `lumen-text` (`shape_cap`, `app.rs`-reachable) | proves or refutes the thrash regime in §5 |
| `glyph_raster_misses` | `lumen-text/src/lib.rs:119` | already counted "for tests/diagnostics" |
| `renderer`, `is_gpu`, `present_mode` | `gpu.rs:594`, shell | one field answers "why is this slow" |
| `frame_ms_max`, `dropped_frames` | `app.rs:1439` (`frame_ms` deque exists) | p95 hides a single 200 ms stall |

`frame_ms` is a 120-entry deque (`app.rs:1213`) — max and an over-budget count
are free from data already collected.

---

## Suggested follow-up shape

1. **A `log!`-style helper on `Headless`** so call sites are one line and the
   level/prefix convention is enforced in one place. Sites in `lumen-render`,
   `lumen-text` and `lumen-shell` have no `Runtime` handle, so they need either
   a passed-in sink or a `Cell`-latched flag the app drains each frame — the
   `atlas_too_big` / `atlas_overflow` pattern (`gpu.rs:2197`) is the precedent
   already in the tree and should be reused rather than reinvented.
2. **Route the shell's ~20 `eprintln!` sites through the ring as well as
   stderr.** Highest ratio of value to risk in this whole list.
3. **Latching/rate-limiting utility**, so "log once per regime change" is not
   re-implemented per site.
4. **Extend `app.perf`** with the table in P3 (pure addition; no existing field
   changes).
5. Per `AGENT.md` doc-currency: any of this that lands must update the
   `.ai_docs/02–05` protocol section for `app.perf`/`app.logs`, the
   `06-task-graph.md` entry, and the `verifying-apps` / `debugging-lumen` skill
   tables that enumerate agent methods.
