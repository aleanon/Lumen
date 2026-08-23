# What the agent cannot see (2026-08-23)

Scope: an app developer building **on** Lumen, with an agent as their hands and
eyes. The question is not "is the framework healthy" but: **what does a human
learn by looking at the running window that the agent currently cannot learn at
all?** Anything identified here may be dev-build-only — `debug_assertions` or a
`dev-observability` feature — because none of it needs to ship in a release
binary.

## The thesis: Lumen's observability is interrogative, not ambient

Lumen's agent surface is genuinely strong, and unusually so: `ui.lint`,
`ui.explain`, `ui.getLayout`, `ui.probe`, `ui.getStyles`, `ui.getDeps`,
`ui.lastChange`. But nearly all of it shares one shape — **you must already
suspect something, and name the node you suspect.** `ui.explain` is the clearest
case: it answers "why didn't `#save` respond to a click" beautifully, and it
answers nothing at all if you didn't know to ask about `#save`.

Human perception is the opposite shape. It is **ambient** (the whole frame at
once), **push** (the anomaly announces itself), and **hypothesis-free** (you
notice the button is missing without having wondered whether it was). A
developer glances at the window and instantly knows: the screen is blank, that
label is cut off, those two panels overlap, nothing moved when I clicked, the
text is white on white.

Every gap below is a case where Lumen *has the data in the frame* and no
mechanism volunteers it. The proposal is therefore not "add more query methods"
— it is **a per-frame ambient audit in dev builds that writes into the existing
`Runtime::log` ring** (`lumen-core/src/state.rs:507`), which the agent already
pages with `app.logs {since}`. That converts pull into push without a new
protocol.

---

## Tier 1 — "I ran it and the screen is blank"

The invisible-but-present family. In every one of these the semantic tree looks
perfectly healthy, so `ui.getTree` actively misleads.

### 1.1 `opacity: 0` (and near-zero) is completely undetectable

`SemanticsNode` (`lumen-core/src/semantics.rs:371`) carries `bounds`, `ink`,
`states`, `text_metrics` — and **no opacity or color of any kind**. `opacity` is
resolved in the cascade (`app.rs:1288`, `:1325`, `:1399`) and consumed by paint;
it never reaches semantics and no audit reads it.

So a node with `opacity: 0` — or an interrupted fade-in stuck at `0.02` — is
invisible on screen, correctly sized in the tree, hit-testable, labelled, and
reported by every existing tool as fine.

Contrast with `visibility: hidden`, which *is* handled properly:
`app.rs:4478-4485` increments `hidden_count`, clears `NodeFlags`, and drops the
node from paint **and semantics**. The agent sees it disappear. `opacity: 0` is
the same user-visible outcome through a path with none of that plumbing.

- **Dev log (warn), per frame, edge-triggered:** node has `opacity <= 0.01`,
  non-zero area, and a `Click` action or non-empty label.
- Add `opacity` (effective, including inherited layer opacity) to
  `SemanticsNode` under the dev feature, so `ui.getTree` stops lying.

### 1.2 Foreground identical to backdrop — white text on white

`analyze_contrast` (`lumen-render/src/analysis.rs:121`) implements APCA properly
and resolves the *composited* backdrop (`resolve_backdrop`, `:165`). It is
wired up as far as `Headless::contrast_report()` (`app.rs:5654`), which even
translates arena indices into agent handles so findings bind to usable
selectors.

**And it is not reachable from the agent protocol.** No `contrast` string
appears anywhere in `lumen-agent/src/`, and `Headless::lint()`
(`app.rs:1787`) never calls it. The only callers are two tests and one example.

This is also a live doc-vs-code drift: `.ai_docs/03-spec-semantics-agent.md:134`
documents `ui.lint` as covering "layout/**contrast** audits … WCAG". It does
not.

- **Wire `contrast_report()` into `ui.lint`** (or a `ui.contrast` method). This
  is the single cheapest item in this document: the analysis, the APCA
  implementation, the backdrop compositing and the handle translation are all
  already written and tested.
- **Dev log (warn):** any text target at `|Lc| < 15` — not a style opinion at
  that threshold, but "this text is invisible".

### 1.3 Nothing checks whether a node is on screen at all

`check_overflow` (`audit.rs:26`) tests:

```rust
b.x1 > p.x1 + 0.5 || b.y1 > p.y1 + 0.5
```

Only the **right and bottom** edges. A node at `x = -400`, or `y = -200`, sits
entirely off the left/top of its parent and raises nothing. Two more
consequences of the same function:

- The whole check is skipped inside any scroll subtree (`in_scroll`, `:22`),
  which is right for the viewport itself but means a node scrolled out of view
  is never mentioned.
- There is no window-viewport check anywhere. Whether the *root* is bigger than
  the surface, or a fixed-position element landed off-canvas, is unasked.

- **Dev log (warn):** a node with a label or a `Click` action whose bounds
  intersect the window rect by **zero area**. "It exists, it is laid out, and
  no part of it is on screen" is the exact sentence a human produces in half a
  second and the agent cannot produce at all.
- Extend `W0103` to the left/top edges while there (`b.x0 < p.x0 - 0.5 || b.y0 < p.y0 - 0.5`).

### 1.4 Occlusion by ordinary siblings

`collect_overlays_at` (`lumen-agent/src/lib.rs:1200`) is the only occlusion
check in the tree. It is narrow by construction, and deliberately so — it
answers one question well:

- it runs **only** inside `ui.explain` with `kind: "click"`;
- it considers **only** nodes with `overlay: true`;
- it tests **only** the single centre point that `input.click` uses.

A plain sibling with a `z-index` (`app.rs:4490`), an absolutely-positioned
panel, or an opaque card that grew over its neighbour will cover a control
completely and be reported by nothing. The human sees one box on top of
another; the agent sees two nodes with overlapping `bounds` and no notion that
overlap means one of them won.

- **Dev log (warn):** an interactive node whose bounds are ≥90% covered by a
  later-painted opaque node. Paint order and z are already resolved at this
  point, so the comparison is available.

### 1.5 The frame is (nearly) blank

No check anywhere asks whether the rendered frame is a single flat colour. This
is the most common early-development outcome — a layout mistake collapses
everything to zero size, a root signal is empty, a build error boundary caught a
panic and kept the last (empty) frame.

`RgbaImage::region_is_uniform` already exists (used by `ui.probeRegion`,
`lumen-agent/src/lib.rs:602`), so the primitive is written.

- **Dev log (warn), once per transition into the state:** ">99% of the frame is
  a single colour" with the colour, plus the node count that *should* have
  painted. A human's first-second reaction, in one line.

### 1.6 The GL adapter silently drops every gradient

Carried over from the framework-internals pass because it belongs squarely in
this tier: `gpu.rs:679-692` documents that when wgpu selects the **GL** backend,
`textureSample` of the gradient ramp returns zeros — *"every gradient in the
frame renders as nothing, with no validation error"*. `Wgpu::new()` (`:678`)
sweeps `PRIMARY` then `SECONDARY`, and `WgpuFallbackTinySkia` (`:586`) drops to
CPU entirely if neither answers.

An app developer whose gradients vanished would see it instantly. Their agent
sees a screenshot it has no baseline for and a tree that is entirely correct.
`is_gpu()` (`:594`) exists and is called only by tests.

- **Dev log (info at startup):** `renderer=<name> backend=<Vulkan|GL|…> adapter="<name>"`.
- **Dev log (warn at startup):** the adapter came from `SECONDARY`/GL — "gradients
  will not render on this backend".

---

## Tier 2 — What is painted disagrees with what the tree says

The agent is not blind here; it is *confidently wrong*, which is worse.

### 2.1 Ellipsised text: the tree returns the full string

This one is intentional and documented as such.
`lumen-widgets/tests/lss_layout_properties.rs:836-841`:

> `text-overflow: ellipsis` paints a truncated string while the SEMANTIC tree
> keeps the full one. That split is the whole feature.

For a11y that is the correct decision. For an agent acting as the developer's
eyes it is a trap with no escape hatch: the screen reads `Quarterly rev…`, the
tree reads `Quarterly revenue by region`, `session.assertText` passes, and the
developer's actual bug — the column is too narrow — is invisible. There is no
field anywhere reporting that truncation occurred.

- **Add `truncated: bool`** (or the painted prefix) to `SemanticsNode` /
  `TextMetrics` under the dev feature. Keep `label` full; report the split
  explicitly rather than making it undiscoverable.
- **Dev log (info):** first time a given node truncates, with the painted vs
  full string.

The same argument applies to text wrapped to more lines than its box allows —
`W0104` (`audit.rs:56`) catches *vertical ink* overflow, and the comment at
`:60` explains that horizontal ink overhang is deliberately ignored to avoid
flagging normal typography. Correct for side bearings; it also means a
horizontally cut-off line is not reported by that path either.

### 2.2 `get_styles` reports the declared value, not the applied one

Recorded in the `W0109` doc comment itself
(`lumen-core/src/diagnostics.rs`, `W0109`):

> `get_styles`, which returns the *declared* value rather than the applied one,
> so a rejected value still appears there.

`W0107`/`W0109` cover this at **parse time** for unimplemented properties and
unusable values, which is a real and well-designed defence. What it does not
cover is the runtime case: a property that parsed fine, applied fine, and was
then overridden by the cascade, a state rule, or a mid-flight transition
(`apply_transitions`, `app.rs:1246`; `apply_keyframes`, `:1338`). The developer
sees a blue button; `ui.getStyles` says `background: red`.

- **Add the *resolved* style** (`node_computed`, already stored at
  `app.rs:4497`) to `ui.getStyles` output alongside the declared one.

### 2.3 Mid-animation values are unreportable

`apply_transitions` / `apply_keyframes` substitute blended values into the
cascade every frame. Neither the blended value nor the fact that a blend is
in flight reaches the agent. `is_animating()` (`app.rs:1743`) and
`next_deadline()` (`:1751`) exist on `Headless` and appear **nowhere** in
`lumen-agent` — `ui.waitSettled` uses the underlying condition but never
reports *what* is animating.

A human sees motion. An agent takes a screenshot mid-transition and compares it
to a golden, with no way to know it caught a frame in flight.

- **Expose `ui.animations`:** in-flight property animations with node, property,
  progress and remaining ms.
- **Dev log (warn):** an animation still running past a generous ceiling (e.g.
  10 s) — the "stuck spinner" signal.

---

## Tier 3 — "Did my change do anything?"

Causality over time. A human sees the screen change; the agent must infer it.

### 3.1 Damage is computed every frame and never exposed

`Headless::last_damage()` (`app.rs:1654`), `FrameStats.damage` (`app.rs:1207`)
and the whole R2 damage-tracking system know precisely which rectangles
repainted. The string `damage` does not occur in `lumen-agent/src/lib.rs` at
all. `Tracer::frame(damage)` in `lumen-test` (`trace.rs:58`) records it for
tests — the live agent path has no equivalent.

"What changed on screen when I clicked that?" is the single most common thing a
human answers by looking, and the data is sitting in a field.

- **Expose `ui.lastDamage`** → rects + the node ids intersecting them.
- **Dev log (info):** on a pump where a handler ran and damage came back
  `Damage::None` — *"the click was handled and repainted nothing"*. That single
  line separates "my handler didn't fire" from "my handler fired and changed
  nothing", which is otherwise a multi-step bisect.

### 3.2 `ui.lastChange` is compiled out of lean builds

`record_change` (`app.rs:2074`) is `#[cfg(feature = "snapshot")]` and becomes
`let _ = (kind, nodes)` otherwise. `snapshot` is on by default, so this is not
usually live — but a `--no-default-features` build (the documented lean profile)
loses the change feed precisely where the agent has fewest alternatives.
Dev-build observability should key off `debug_assertions` or its own feature,
not off `snapshot`.

### 3.3 Nothing reports a signal write that changed no pixels

`pump()` (`app.rs:1056`) already computes everything needed: `write_changed`,
`structural_current`, `visual_changed`, `time_driven`, and which of the five
branches ran (rebuild / restyle-only / patch-text / patch-bg / idle). A pump
where `write_changed` is true and the frame was declared idle is the canonical
stale-UI bug — and it is discarded.

- **Dev log (warn):** signal(s) `{keys}` were written and the frame took the
  idle branch, naming the predicate that vetoed the rebuild. `dependents_of`
  (`app.rs:3771`) can name the signals.

### 3.4 A resource that never resolves

`lumen-app/src/tasks.rs:110` `finish()` stores `Err(e)` on the resource cell for
the view to render; if the view doesn't render errors (common early on), the
failure is silent. And a fetch that simply never returns leaves `is_ready()`
false forever — a human watches a spinner spin and concludes something is wrong
within seconds.

- **Dev log (warn):** any `TaskError`, mirrored into the ring regardless of
  whether the view displays it.
- **Dev log (warn):** a resource pending beyond a threshold (e.g. 10 s).
- **Dev log (warn):** the same task key superseded N times in a short window —
  a dep that changes every frame, cancelling and respawning forever
  (`tasks.rs:202`, `:361`).
- `tasks.rs:351` falls back to *"an inert slot rather than panicking"* when the
  runtime is unavailable. A task that will never run and never says so.

### 3.5 Deferred results land invisibly

`drain_deferred()` (`lumen-core/src/tasks.rs:275`) returns a count that
`app.rs:1068` discards. "Your data arrived on frame N" is a one-line `info` and
distinguishes "the fetch never completed" from "the fetch completed and the view
ignored it".

---

## Tier 4 — Jank a human feels

Perceived performance, as opposed to the averages `app.perf` reports today
(`frame_ms_p50`, `frame_ms_p95`, `frames_rendered`, `node_count`).

A p95 of 6 ms with one 300 ms stall reads as healthy and feels broken. The
`frame_ms` deque (`app.rs:1213`, 120 entries) already holds what's needed for
`frame_ms_max` and an over-budget count.

Two per-frame regressions are measurable, invisible, and already half-tracked:

- **Text cache thrashing.** `sweep()` (`lumen-text/src/lib.rs:379`) documents a
  measured **2.2× frame-time penalty** (3.8 → 8.5 ms; 1183 re-shapes/frame at
  2000 rows) and emits nothing. Log on *entry to the regime* — the hard-cap
  fallback at `:388` — not per sweep.
- **Retained-pipeline effectiveness.** `nodes_rebuilt` / `nodes_copied`
  (`app.rs:801`) and `style_memo_hits` / `style_memo_misses` (`app.rs:808`) are
  maintained every frame; `style_memo_stats()` (`app.rs:1432`) is a public
  accessor **with no callers**. A copy rate near zero means memoization is off,
  which is the difference between the CP1–CP6 work applying and not.

`W0108` (`audit.rs:200`) deserves a note as the model the rest of this document
is arguing for. Its doc comment says it plainly:

> A lint is how the framework says so at the moment it matters, to an author
> (or an agent) who cannot see a frame budget.

That is exactly right, and it is the only check in the codebase built on that
premise. Everything above is the same argument applied to the things a human
sees rather than the things a human profiles.

---

## Delivery

1. **One dev-only ambient audit, run per frame, writing to `Runtime::log`.**
   No new protocol; `app.logs {since}` already pages it. Gate on
   `debug_assertions` or a `dev-observability` feature — explicitly *not* on
   `snapshot` (see 3.2).
2. **Edge-triggered, always.** Log the transition into a state, not the state.
   At 120 fps an unconditional line flushes the 1000-entry ring in 8 seconds.
   The existing `W0107`/`W0109` "once per declaration at parse time" discipline
   and the `atlas_overflow` / `atlas_too_big` `Cell`-latch pattern
   (`gpu.rs:2197`) are the precedents to copy.
3. **Sites outside `lumen-app` have no `Runtime` handle.** `lumen-render`,
   `lumen-text` and `lumen-shell` should latch a flag the app drains each frame
   — again the `take_diagnostics()` pattern already used for `W0110`
   (`app.rs:1789`).
4. **Route `lumen-shell`'s ~20 `eprintln!` sites into the ring as well.** Under
   `just run-agent` the agent reads a socket; stderr goes to the developer's
   terminal. Renderer name (`shell/lib.rs:131`), present mode (`:616`), the
   permanent direct→readback degradation (`:952`), reload results (`:508`) and
   window failures (`:1076`, `:1085`) are all agent-relevant and all
   agent-invisible.
5. **Cheapest first:** wire the existing `contrast_report()` into `ui.lint`
   (1.2), expose `last_damage()` (3.1), and call `style_memo_stats()` from
   `app.perf` (Tier 4). Three already-written, already-tested capabilities that
   currently have no caller.
6. Per `AGENT.md` doc-currency: anything landing here updates
   `.ai_docs/03-spec-semantics-agent.md` (including the incorrect `ui.lint`
   contrast claim on line 134), the `06-task-graph.md` entry, and the
   `verifying-apps` / `debugging-lumen` skill method tables.
