# Plan: agent observability — making the running app legible without eyes

*Build plan, 2026-08-23. Companion to `review-agent-observability-2026-08.md`,
which is the evidence; this file is the work.*

**Goal.** An agent helping someone develop a Lumen app should learn, without a
screenshot and without a hypothesis, the things a human learns by glancing at
the window: the screen is blank, that label is cut off, those panels overlap,
nothing moved when I clicked, the text is invisible.

**Budget.** Dev builds only. Nothing here needs to ship in a release binary, so
cost is bounded by "does it slow down a debug session", not by the release
frame budget.

---

## The organizing idea

Two mechanisms, split on a principle rather than on convenience:

| | `ui.lint` findings (`W####`) | `app.logs` ring entries |
|---|---|---|
| Answers | "what is wrong with **this frame**" | "what **happened** over time" |
| Anchored to | a node | a moment |
| Shape | idempotent, re-derivable | append-only, sequenced |
| Examples | opacity 0, off-screen, occluded | click repainted nothing, task errored, cache regime changed |

The bridge between them — and the single highest-leverage piece of this plan —
is **O0.3**: in dev builds, run `lint()` every frame and push *newly appeared*
findings into the log ring. That converts every lint that exists today and
every lint added tomorrow from pull to push, with no protocol change and no
per-finding wiring. `ui.lint` stays exactly as it is for callers who want to
poll.

## Diagnostic codes allocated

Per `crates/lumen-core/diagnostics.md` ("next free" is W0111 / W0303 / W0403).
Allocate in O0.1 so parallel work cannot collide:

| Code | Meaning | Task |
|---|---|---|
| W0111 | Node is effectively transparent (`opacity ≤ 0.01`) but present, sized and interactive/labelled | O2.1 |
| W0112 | Node is laid out entirely outside the window viewport | O2.2 |
| W0113 | Interactive node is ≥90% covered by a later-painted opaque node | O2.3 |
| W0114 | Frame is effectively blank — >99% one colour with a non-trivial node count | O2.4 |
| W0115 | Active renderer backend has known rendering defects (GL drops gradients) | O2.5 |
| W0403 | Text is painted truncated (`text-overflow: ellipsis`); the semantic label is the full string | O3.1 |

W0116+ / W0404+ stay free. Update the registry's "next free" block in the same
commit.

---

# Phase O0 — Foundations

Everything else depends on these three. Land them first, in one commit each.

### O0.1 — The `dev-observability` feature

**Goal.** One switch, on by default in debug builds, that gates every addition
in this plan.

**Files.** `crates/lumen-app/Cargo.toml`, `crates/lumen-core/Cargo.toml`,
`crates/lumen-widgets/Cargo.toml`, `crates/lumen/Cargo.toml`.

**Approach.**

- Add `dev-observability` to `lumen-app` and `lumen-core`, forwarded up through
  `lumen-widgets` → `lumen`. Default **on** (matching `snapshot`), so
  `cargo run` gets it and `--release --no-default-features` does not.
- Gate the *runtime cost* on `cfg(any(debug_assertions, feature = "dev-observability"))`
  so a release build with the feature explicitly enabled still works — some
  developers profile in release.
- **Do not gate on `snapshot`.** `record_change` (`app.rs:2074`) does exactly
  that today and consequently compiles out of lean builds, which is where the
  agent has fewest alternatives (see O4.3).
- Allocate the six codes above in `lumen-core/src/diagnostics.rs` + the registry
  table + the "next free" block, all in this commit (ADR-019 requires the const
  and the row together).

**Acceptance.** `cargo build --no-default-features -p lumen-app` compiles with
zero observability code linked; `cargo build` links it. A test asserts the
codes exist and that severities agree with their leading letter.

### O0.2 — Latch and rate-limit primitives

**Goal.** "Log this once per regime, not once per frame" written once.

**Why.** At 120 fps an unconditional line flushes the 1000-entry ring
(`state.rs:507`) in eight seconds. Every site in this plan is edge-triggered,
and without a shared primitive each one will re-invent it slightly differently.

**Files.** New `crates/lumen-core/src/observe.rs`, exported from `lib.rs`.

**Approach.** Three small types:

- `Latch` — `set(bool) -> bool`, returning `true` only on a `false → true`
  transition. For "entered a degraded regime".
- `SeenSet` — a `HashSet` of finding keys; `insert_new()` returns only keys not
  seen before. This is what O0.3 uses to diff lint findings frame over frame,
  and what makes "the same W0111 on the same node" quiet after the first frame.
- `Throttle` — first N occurrences then every Nth, keyed by call site. For
  per-frame anomalies that are legitimately recurrent (`Present::Skipped`
  during a resize drag).

Precedent in tree to follow rather than reinvent: the `Cell`-latched
`atlas_overflow` / `atlas_too_big` pair (`gpu.rs:2197`) and
`Renderer::take_diagnostics` (`lumen-render/src/lib.rs:175`) — that is exactly
the "a crate with no `Runtime` handle latches, the app drains" shape that
O2.5 and O5.2 need.

**Acceptance.** Unit tests per type. A test that 10 000 identical `SeenSet`
insertions produce one entry.

### O0.3 — The ambient audit ★

**Goal.** Findings announce themselves instead of waiting to be asked for.

**This is the load-bearing task.** Everything in O2 and O3 is worth roughly
nothing without it, because a finding nobody polls for is a finding nobody
sees.

**Files.** `crates/lumen-app/src/app.rs` (`pump`, ~`:1200`, after the branch
resolves and before `FrameStats` is built).

**Approach.**

```
if cfg!(dev-observability) && frame_painted {
    for d in self.lint() {
        if seen.insert_new(key(&d)) {           // O0.2
            self.rt.log(level_of(d.severity), format!("{d}"));
        }
    }
}
```

- **Only on painted frames.** An idle pump changes nothing, so re-linting it is
  pure waste; `stats.painted` is already computed at `app.rs:1207`.
- **Key on `(code, node, message)`**, not on the `Diagnostic` — the message
  carries measured values (`"overflows its parent (12×0 past the edge)"`) that
  jitter during an animation and would defeat deduplication if hashed whole.
  Key on code + node id, and log the message.
- **Clear the `SeenSet` on `rebuild_fresh()`** (`app.rs:2754`) and on stylesheet
  /theme change, so a fixed problem that returns is reported again.
- **Guard the cost.** `lint()` walks the tree and, in the tofu path
  (`app.rs:1846-1868`), re-lays out every text node. Measure before/after on
  `benches-competitive`; if a debug frame regresses more than ~2×, split the
  expensive checks behind an every-Nth-frame cadence rather than dropping them.

**Acceptance.** A headless test: build an app with an overflowing child, pump
twice, assert `app.logs` contains exactly **one** `W0103` entry. Fix the
overflow, pump, re-break it, assert a second entry appears.

**Risk.** This is the one task in the plan that can plausibly slow a debug
session. Land it with the measurement in the commit message.

---

# Phase O1 — Already built, never called

Three capabilities that are written, tested, and have no caller. Highest value
per line changed in the whole plan; no new concepts.

### O1.1 — Wire contrast into `ui.lint`

**Goal.** White-on-white text becomes detectable.

**Files.** `crates/lumen-app/src/app.rs` (`lint`, `:1787`),
`crates/lumen-agent/src/lib.rs` (`ui.lint`, `:590`),
`.ai_docs/03-spec-semantics-agent.md:134`.

**Approach.** `contrast_report()` (`app.rs:5654`) already runs APCA against the
*composited* backdrop (`resolve_backdrop`, `analysis.rs:165`) and already
translates arena indices to agent handles so findings bind to usable selectors.
Call it from `lint()` and map findings below a floor into diagnostics.

- Use a **legibility floor, not a design opinion**: `|Lc| < 15` is "this text is
  invisible", not "this text is low-contrast". Design-grade thresholds stay in
  `ContrastLevel` for callers who want them (`analysis.rs:45`).
- Note the documented limitation at `analysis.rs:163`: gradients and images do
  not contribute to the backdrop. Text over a gradient will not be assessed —
  state that in the finding's absence rather than pretending coverage.
- **`.ai_docs/03-spec-semantics-agent.md:134` currently claims `ui.lint` covers
  "layout/contrast audits … WCAG" and it does not.** This task makes the
  documentation true; that line is the acceptance criterion, not a follow-up.

**Acceptance.** A test with `#a { color: white; background: white }` produces a
contrast finding through `ui.lint`; the same text on black produces none.

### O1.2 — Expose damage as `ui.lastDamage`

**Goal.** "What changed on screen when I clicked that?" — the question a human
answers by looking, whose answer is already sitting in a field.

**Files.** `crates/lumen-agent/src/lib.rs` (new method),
`.ai_docs/03-spec-semantics-agent.md` (method table).

**Approach.** `Headless::last_damage()` (`app.rs:1654`) and `FrameStats.damage`
(`:1207`) are computed every frame; the string `damage` does not appear in
`lumen-agent/src/lib.rs` at all. Return the rects **plus the node handles whose
bounds intersect them** — rects alone would make the agent do a spatial join it
has no primitive for.

Precedent for the shape: `Tracer::frame(damage)` in `lumen-test`
(`trace.rs:58`) already records damage for tests; match its field naming.

**Acceptance.** Click a toggle; `ui.lastDamage` returns a rect containing the
toggle's bounds and its node handle. Pump an idle frame; it returns empty.

### O1.3 — Complete `app.perf`

**Goal.** Stop hiding the numbers that decide whether the retained pipeline is
working.

**Files.** `crates/lumen-agent/src/lib.rs` (`app.perf`, `:614`),
`crates/lumen-app/src/app.rs` (accessors).

**Approach.** Pure addition to the response object — no existing field changes:

| Field | Source | Why |
|---|---|---|
| `nodes_rebuilt`, `nodes_copied` | `FrameStats`, `app.rs:1209` | copy rate ≈ 0 ⇒ memoization is off |
| `style_memo_hits`, `style_memo_misses` | `style_memo_stats()`, `app.rs:1432` — **public accessor, zero callers** | A.5b restyle effectiveness |
| `frame_ms_max`, `frames_over_budget` | the 120-entry `frame_ms` deque, `app.rs:1213` | p95 of 6 ms with one 300 ms stall reads healthy and feels broken |
| `renderer`, `is_gpu`, `backend` | `is_gpu()`, `gpu.rs:594` — also caller-less | one field answers "why is this slow" |
| `shape_cache_len`, `shape_cache_cap` | `lumen-text` engine state | proves or refutes the thrash regime (O5.2) |

`frame_ms_max` and `frames_over_budget` are free — the deque already holds them.

**Acceptance.** Schema test on the response; a test that a memoized rebuild
reports `nodes_copied > 0` and a forced full rebuild reports `nodes_copied == 0`.

---

# Phase O2 — "I ran it and the screen is blank"

The invisible-but-present family. In each of these the semantic tree looks
healthy, so `ui.getTree` actively misleads.

### O2.1 — Opacity reaches semantics (W0111)

**Goal.** A node faded to nothing stops reporting itself as fine.

**Files.** `crates/lumen-core/src/semantics.rs` (`SemanticsNode`, `:371`),
`crates/lumen-app/src/app.rs` (`build_semantics_at`, `:5700`; `NodeMeta`, `:590`),
`crates/lumen-app/src/audit.rs` (new check).

**Approach.**

- `SemanticsNode` carries `bounds`, `ink`, `states`, `text_metrics` — and no
  opacity or colour at all. Add `opacity: Option<f32>` (`None` ⇒ fully opaque,
  keeping the serialized tree unchanged for the common case).
- Store the **effective** opacity — the node's own multiplied by enclosing layer
  opacity — not the declared one. A node at `opacity: 1` inside a group at
  `opacity: 0` is equally invisible. The cascade resolves opacity at
  `app.rs:1399`; the layer product is what paint uses.
- W0111 fires when effective opacity `≤ 0.01` **and** the node has non-zero area
  **and** it is interactive or labelled. Decorative fades must stay quiet or the
  check will be ignored.
- **Exempt nodes with a running opacity transition** — `span_has_running_anim`
  (`app.rs:3799`) already answers this. A fade-in passing through zero on frame
  one is not a defect; a fade that *stopped* at zero is, and O3.3's stuck-
  animation check is what catches that.

**Design note.** `visibility: hidden` is already handled correctly —
`app.rs:4478` increments `hidden_count`, clears `NodeFlags`, and the node leaves
paint *and* semantics (`build_semantics_at:5753`), with the comment "what the
agent sees matches what the user sees". `opacity: 0` is the same user-visible
outcome through a path with none of that plumbing. This task makes the two
consistent, but deliberately **not** identical: hidden nodes vanish from the
tree, transparent ones stay and are flagged, because opacity is animatable and a
node oscillating in and out of the tree would churn identity every frame.

**Acceptance.** `#a { opacity: 0 }` on a button ⇒ W0111 and `opacity: 0.0` in
the tree. Mid-transition ⇒ no finding. Inside an `opacity: 0` group ⇒ finding
on the child too.

### O2.2 — Off-screen detection (W0112) and the W0103 edge bug

**Goal.** "It exists, it is laid out, and no part of it is on screen."

**Files.** `crates/lumen-app/src/audit.rs` (`check_overflow`, `:26`; new check).

**Approach.** Two separable pieces; land them together since they are the same
function.

1. **Fix `check_overflow`.** It tests `b.x1 > p.x1 + 0.5 || b.y1 > p.y1 + 0.5`
   — right and bottom edges only. A node at `x = -400` is entirely off the left
   of its parent and raises nothing. Add `b.x0 < p.x0 - 0.5 || b.y0 < p.y0 - 0.5`.
   *This changes existing behaviour*: previously-silent apps will start
   reporting W0103. Expect golden/test churn and treat new findings as real
   until proven otherwise.
2. **Add W0112.** A node with a label or a `Click` action whose bounds intersect
   the **window rect** by zero area. Distinct from W0103, which is parent-
   relative: a node can sit correctly inside a parent that is itself off-screen.
   Exempt overlay roots (they anchor to the window by design, `audit.rs:27`) and
   nodes inside a scroll subtree (scrolled out of view is not a defect) — but
   see the note below.

**Scroll caveat.** `check_overflow` exempts the entire subtree under any scroll
container (`in_scroll`, `audit.rs:22`). That is right for the viewport itself
and means content scrolled out of view is never mentioned. Rather than weaken
the exemption, O2.2 leaves it and relies on `ScrollInfo` (`semantics.rs:327`,
which already carries `x/y/max_x/max_y`) — the agent *can* derive "there is
more below". Consider a follow-up `info` log volunteering "12 of 400 rows
visible" if that proves insufficient in practice.

**Acceptance.** A node at `x: -500` ⇒ W0103 (left edge) and W0112. A node inside
a scrolled list ⇒ neither.

### O2.3 — General occlusion (W0113)

**Goal.** An opaque sibling covering a control is reported.

**Files.** `crates/lumen-app/src/audit.rs` (new check), reusing paint order from
`crates/lumen-app/src/app.rs` (`build_display_list`, `:4770` — `tree.paint_order()`).

**Approach.** `collect_overlays_at` (`lumen-agent/src/lib.rs:1200`) is the only
occlusion check today, and it is narrow by construction: only inside
`ui.explain`, only `overlay: true` nodes, only the single click centre-point.
A plain `z-index` sibling (`app.rs:4490`), an absolutely-positioned panel, or an
opaque card that grew over its neighbour is reported by nothing.

- Walk `tree.paint_order()` — already computed per frame — and for each
  interactive node, test coverage by nodes painted **later** that are opaque
  (background alpha 1, no transparency, and after O2.1 effective opacity 1).
- Threshold at ≥90% area coverage. Partial overlap is routine layout; near-total
  coverage means the control cannot be reached.
- **Cost:** naive is O(n²). Bound it by testing only interactive nodes (a small
  subset) against later siblings whose bounds intersect, and skip entirely past
  a node-count ceiling. Log the skip — a silent cap reads as "covered
  everything".

**Acceptance.** Two overlapping cards, the later opaque ⇒ W0113 on the earlier
button. Later card at `opacity: 0.5` ⇒ no finding. `ui.explain` unchanged.

### O2.4 — Blank-frame detection (W0114)

**Goal.** The most common early-development outcome gets a first-second answer.

**Files.** `crates/lumen-app/src/app.rs` (`lint`), using
`RgbaImage::region_is_uniform` (already used by `ui.probeRegion`,
`lumen-agent/src/lib.rs:602`).

**Approach.** If >99% of the frame is one colour **and** the tree has a
non-trivial node count, report W0114 with the colour and the node count that
should have painted. The primitive exists; this is a sampling policy on top of
it.

- **Sample, don't scan.** A full-frame uniformity test per frame is a debug-
  build cost nobody needs. A coarse grid (e.g. 32×32 probes) catches the case
  with certainty in practice and is O(1) in frame size.
- Suppress on the very first frame (a legitimately empty initial state) and
  while an entry transition is running.
- Requires a rendered frame, so it belongs in the `lint()` path rather than in
  `audit.rs`, which is a pure semantics walk.

**Acceptance.** An app whose root collapses to zero size ⇒ W0114. A normal app
⇒ none. A deliberately single-colour splash screen with two nodes ⇒ none (node
count below the threshold).

### O2.5 — Renderer identity and the GL gradient defect (W0115)

**Goal.** The developer whose gradients vanished gets told why.

**Files.** `crates/lumen-render/src/gpu.rs` (`Wgpu::new`, `:678`;
`WgpuFallbackTinySkia::new`, `:586`), `crates/lumen-app/src/app.rs` (drain),
`crates/lumen-shell/src/lib.rs:131`.

**Approach.** `gpu.rs:679-692` documents that on the **GL** backend
`textureSample` of the gradient ramp returns zeros — *"every gradient in the
frame renders as nothing, with no validation error"*. `Wgpu::new()` sweeps
`PRIMARY` then `SECONDARY`; `WgpuFallbackTinySkia` (`:586`) drops to CPU
entirely if neither answers. `is_gpu()` (`:594`) exists and only tests call it.

- **Log (info, once at startup):** `renderer=<name> backend=<Vulkan|Metal|GL|…> adapter="<name>"`.
- **W0115 (warn, once):** the adapter came from `SECONDARY`/GL — name the
  consequence ("gradients will not render on this backend"), not just the fact.
- **Log (warn, once):** no adapter at all ⇒ CPU renderer, with the performance
  consequence stated.
- `lumen-render` has no `Runtime` handle: latch at construction and drain
  through `take_diagnostics()` (`lumen-render/src/lib.rs:175`), the pattern
  W0110 already uses.

**Cross-reference.** `docs/gl-gradient-defect.md` and the
`gl-backend-gradient-defect` memory both record this; the fix here is only to
make it *observable*, not to change backend selection.

**Acceptance.** Force `WGPU_BACKEND=gl` ⇒ W0115 present. Default ⇒ absent, and
an info line names the real backend.

---

# Phase O3 — What is painted disagrees with what the tree says

The agent is not blind here; it is confidently wrong, which is worse.

### O3.1 — Truncation is reported (W0403)

**Goal.** `assertText` stops passing on text the user cannot read.

**Files.** `crates/lumen-app/src/app.rs` (`NodeMeta.display_text`, `:635`;
`build_semantics_at`, `:5700`), `crates/lumen-core/src/semantics.rs`
(`TextMetrics`, `:347`).

**Approach.** This is nearly free, because the signal is already stored.
`NodeMeta.display_text: Option<String>` (`app.rs:635`) holds *"the truncated
string the PAINT pass draws"* — `Some(_)` **is** the truncation flag. Its doc
comment states the design decision plainly:

> The node's own text (and therefore the semantic tree, the agent and assistive
> tech) keeps the FULL string — truncating that would make `ui.getTree` report
> "Some long lab…", corrupting the observability surface to fix a visual one.

That reasoning is correct and this task does not reverse it. It adds the
*missing third option*: keep `label` full, and report the split explicitly.

- Add `painted_text: Option<String>` to the node (or `truncated: bool` +
  `painted_text` on `TextMetrics`). Populate from `display_text`.
- W0403 at `info`/`warning` severity — truncation is often intentional, so this
  is advisory. The value is that it becomes *knowable*, not that it is wrong.
- Same treatment for the horizontal-clip case: `W0104` (`audit.rs:56`)
  deliberately ignores horizontal ink overhang (`audit.rs:60`) to avoid flagging
  ordinary side bearings — correct, and it means a horizontally cut-off line
  without `text-overflow` set is also unreported. Cover it here via the same
  painted-vs-full comparison rather than by loosening W0104.

**Acceptance.** The existing test at
`lumen-widgets/tests/lss_layout_properties.rs:841` keeps passing unchanged
(`label` is still the full string) **and** the tree now reports
`painted_text: "Quarterly rev…"`.

### O3.2 — Applied vs computed styles

**Goal.** `ui.getStyles` stops reporting a colour the node is not currently
painted in.

**Files.** `crates/lumen-app/src/app.rs` (`get_styles`, `:4020`).

**Approach.** Be precise about what the gap actually is, because the first
draft of this review overstated it. `get_styles` reads `node_computed`
(`app.rs:4030`), which **is** the resolved cascade result with origin and span —
not the raw declaration. Two real gaps remain:

1. **Transition/keyframe blends.** `apply_transitions` (`:1246`) and
   `apply_keyframes` (`:1338`) mutate `css` *before* the split at `:4497`
   (`node_style` ← `css`, `node_computed` ← `resolved`). `get_styles` reads
   `node_computed`, so mid-transition it reports the **target** value while the
   node paints the blend.
2. **Runtime-rejected values.** Recorded in the `W0109` doc comment itself:
   *"`get_styles` … returns the declared value rather than the applied one, so
   a rejected value still appears there."* W0107/W0109 catch this at parse time,
   which covers the authoring case but not introspection.

Add an `applied` object alongside the existing computed one, sourced from
`node_style`. Additive — existing keys keep their meaning and shape.

**Acceptance.** Mid-transition, `get_styles` reports computed = target and
applied = blend. A `W0109`-rejected value appears in computed and is absent from
applied.

### O3.3 — `ui.animations` and the stuck-animation warning

**Goal.** A human sees motion, and sees when motion never stops.

**Files.** `crates/lumen-agent/src/lib.rs` (new method),
`crates/lumen-app/src/app.rs` (`is_animating`, `:1743`; `next_deadline`, `:1751`;
`PropAnim`, `:986`).

**Approach.** `is_animating()` and `next_deadline()` are on `Headless` and
appear **nowhere** in `lumen-agent`. `ui.waitSettled` (`lumen-agent:642`) uses
the underlying condition but never reports *what* is animating, so an agent that
screenshots mid-transition and diffs against a golden has no way to know it
caught a frame in flight.

- `ui.animations` → `[{ node, property, progress, remaining_ms }]`, from the
  `PropAnim` table (`app.rs:986-1010`, which already has `progress()` and
  `done()`).
- **Log (warn, latched):** an animation still running past a generous ceiling
  (10 s). This is the "stuck spinner" signal, and it is also the escape hatch
  for O2.1's transition exemption — a fade stuck at zero is caught here.

**Acceptance.** During a 300 ms transition, `ui.animations` is non-empty with
`0 < progress < 1`; after `ui.waitSettled`, empty. An `animation: spin infinite`
⇒ one warning at 10 s, not one per frame.

---

# Phase O4 — "Did my change do anything?"

Causality over time.

### O4.1 — Handled-but-no-damage

**Goal.** Separate "my handler didn't fire" from "my handler fired and changed
nothing" — otherwise a multi-step bisect.

**Files.** `crates/lumen-app/src/app.rs` (`pump`, `:1200`; `route`, `:2143`).

**Approach.** `route` already knows whether a handler ran (`did_click`,
`did_focus`, `did_drag` — `app.rs:2195`, all three can end false and all three
are discarded). Combine with `last_damage`:

- **warn:** a handler ran and damage came back `Damage::None`.
- **warn:** a pointer-down bubbled to the root with **no** handler at all, with
  the coordinates. This is `input.click`'s missing half — `lumen-agent:748`
  returns `{"ok": true}` whenever the *selector* resolved, regardless of whether
  anything was hit.
- **warn:** the target was disabled (`is_disabled`, `:2574`), or the press was
  taken by an overlay/scrim (`dismiss_outside`, `:2441`) — currently
  indistinguishable from a working click.

**Deliberately not changing** `input.click`'s return shape: agents and exported
tests depend on it. The information lands in the ring instead.

**Acceptance.** Click a `Text` with no handler ⇒ one warn naming the coordinates.
Click a working button ⇒ none.

### O4.2 — Written-but-idle

**Goal.** The canonical stale-UI bug reports itself.

**Files.** `crates/lumen-app/src/app.rs` (`pump`, the `else` idle arm, `:1183`).

**Approach.** `pump` already computes everything needed — `write_changed`,
`structural_current`, `visual_changed`, `time_driven` (`:1096-1116`) — and which
of the five branches ran. A pump where `write_changed` is true and the frame
took the idle branch is the bug; today it is discarded.

Log at `warn`, naming **which predicate vetoed the rebuild**, and name the
signals via `dependents_of` (`app.rs:3771`). "State changed but the UI is stale"
is the top entry in the `debugging-lumen` skill and currently has no
machine-readable trace at all.

**Acceptance.** A signal written from outside any view (no dependents) ⇒ one
warn naming it. A normal state change ⇒ none.

### O4.3 — Decouple `record_change` from `snapshot`

**Goal.** The change feed survives the lean build.

**Files.** `crates/lumen-app/src/app.rs` (`record_change`, `:2074`;
`last_change`, `:4092`).

**Approach.** `record_change` is `#[cfg(feature = "snapshot")]` and becomes
`let _ = (kind, nodes)` otherwise. `snapshot` is on by default so this is
usually live — but a `--no-default-features` build (the documented lean profile)
loses `ui.lastChange` precisely where the agent has fewest alternatives.
Re-gate on `dev-observability` (O0.1). The `nodes` payload needs
`handle_for_index`, which does not require `serde_json`; only the JSON
*serialization* in `last_change` does.

**Acceptance.** `cargo test -p lumen-app --no-default-features --features dev-observability`
exercises `last_change`.

### O4.4 — Task lifecycle

**Goal.** A spinner that spins forever explains itself.

**Files.** `crates/lumen-app/src/tasks.rs` (`finish`, `:110`; `resource_impl`,
`:180`; `task_impl`, `:336`).

**Approach.** Four logs, each closing a distinct silent failure:

- **warn — every `TaskError`.** `finish()` (`:127`) stores `Err(e)` on the
  resource cell for the view to render; if the view doesn't render errors
  (common early on), the failure is invisible. Mirror it into the ring
  regardless.
- **warn — a resource pending past a threshold** (10 s). A human watches a
  spinner and concludes something is wrong in seconds.
- **warn — the same task key superseded N times in a short window.** A dep that
  changes every frame cancels and respawns forever (`:202`, `:361`); the app
  looks like it is loading and never will be.
- **warn — the inert-slot fallback** at `:351`, which the comment describes as
  *"Fall back to an inert slot rather than panicking"*. A task that will never
  run and never says so.

**Acceptance.** A resource whose fetcher returns `Err` ⇒ a ring entry even with
no error UI. A resource keyed on a per-frame-changing dep ⇒ one supersession
warning, not one per frame.

### O4.5 — Deferred results land visibly

**Goal.** Distinguish "the fetch never completed" from "it completed and the
view ignored it".

**Files.** `crates/lumen-core/src/tasks.rs` (`drain_deferred`, `:275`),
`crates/lumen-app/src/app.rs:1068`.

**Approach.** `drain_deferred()` returns a count that `pump` discards. Log at
`info` when it applies > 0 ops. One line, one call site.

**Acceptance.** A `ManualSpawner` task completing ⇒ one info line naming the
frame.

---

# Phase O5 — Jank a human feels

### O5.1 — Outlier frames

Folded into **O1.3** (`frame_ms_max`, `frames_over_budget`) — the 120-entry
`frame_ms` deque already holds them. Listed here so the tier is complete.

### O5.2 — Text-cache regime change

**Goal.** A measured 2.2× frame-time penalty stops being silent.

**Files.** `crates/lumen-text/src/lib.rs` (`sweep`, `:379`; call sites `:771`,
`:829`).

**Approach.** `sweep`'s own doc comment records the measurement: **1183
re-shapes per frame** and 1.16 evictions per frame at 2000 rows, for a 2.2×
penalty (3.8 → 8.5 ms). It emits nothing.

- **warn, latched on entry to the regime:** the hard-cap fallback at `:388`
  (`map.len() >= hard_cap`, the drop-half path). Name `VirtualList` as the fix,
  the way W0108 does.
- **info, on each cap doubling:** the retarget at `:396`. "Your working set
  outgrew the cache" — not yet a problem, and the leading indicator of one.
- **Not per sweep.** Sweeps are routine; regime changes are not.
- No `Runtime` handle in `lumen-text`: latch and drain, as in O0.2/O2.5.

**Acceptance.** The existing thrash test
(`lumen-text/src/lib.rs:1522-1555`, which already constructs the lock-in
condition) additionally asserts exactly one warning.

### O5.3 — Shell events reach the ring

**Goal.** Stop writing the agent's most useful facts to a stream it cannot read.

**Files.** `crates/lumen-shell/src/lib.rs` (~20 `eprintln!` sites).

**Approach.** Under `just run-agent` the agent reads a socket; stderr goes to
the developer's terminal. Keep the `eprintln!` (the human wants it too) and
**additionally** log to the ring:

| Site | Fact | Level |
|---|---|---|
| `:131` | `renderer = <name>` | info |
| `:616` | `present = direct-to-surface \| cpu-readback` | info |
| `:952` | `Present::Unavailable` ⇒ **permanent** per-frame readback for the session | warn |
| `:940` | `Present::Skipped` — throttled past ~8 consecutive (routine during a resize drag) | warn |
| `:508`/`:510` | stylesheet reload ok / rejected | info / warn |
| `:1076`/`:1085` | window has no declaration / failed to open | warn |
| `:1335`/`:1356` | tray init failures | warn |
| `:2222` | notification fell back to stderr | info |

Also **O5.3b:** `app.rs:3945` `set_stylesheet` logs the *rejection* but not the
acceptance, so an agent cannot distinguish "reload applied" from "the watcher
never fired". Add the success line.

**Acceptance.** With the agent attached, `app.logs` contains the renderer and
present-mode lines before the first user interaction.

---

# Phase O6 — Documentation currency

Required by `AGENT.md` (plan D0.7) **in the same commit as each change**, not as
a trailing pass. Listed once here; each task above inherits it.

- **`.ai_docs/03-spec-semantics-agent.md`** — method table gains `ui.lastDamage`
  (O1.2), `ui.animations` (O3.3), the extended `app.perf` fields (O1.3), the
  `SemanticsNode` additions (O2.1, O3.1). **Line 134's contrast claim becomes
  true in O1.1** — that is a correction, not an addition.
- **`.ai_docs/02-spec-core.md §9`** — the six new W-codes.
- **`crates/lumen-core/diagnostics.md`** — six rows plus the "next free" block
  (O0.1).
- **`.ai_docs/06-task-graph.md`** — an O-phase entry with ◐/☑ per task.
- **`.claude/skills/verifying-apps`** — the method table gains the new methods.
- **`.claude/skills/debugging-lumen`** — symptom→tool mapping updated: several
  entries currently route to a screenshot and should route to `app.logs`.
- **`.claude/skills/styling-lss`** — the property table if O3.2 changes what
  `get_styles` returns.

---

# Ordering and dependencies

```
O0.1 feature ─┬─ O0.2 latches ─┬─ O0.3 ambient audit ★
              │                │
              │                └─ everything in O2/O3 becomes push
              │
              └─ O1.1 contrast ─┐
                 O1.2 damage    ├─ independent, ship first
                 O1.3 app.perf ─┘

O2.1 opacity ──> O2.3 occlusion (needs effective opacity to judge "opaque")
O2.1 opacity ──> O3.3 animations (stuck-fade exemption)
O1.2 damage  ──> O4.1 handled-but-no-damage
```

**Suggested sequence.**

1. **O1.1–O1.3** first. No new concepts, three already-tested capabilities that
   currently have no caller, and O1.1 fixes a false statement in the spec.
   Immediately useful even if the rest of the plan stalls.
2. **O0.1–O0.3** next. O0.3 is the multiplier; land it before writing new
   checks, so each new check is push-mode from birth.
3. **O2** — the blank-screen family, in order (O2.1 gates O2.3).
4. **O4.1/O4.2** — highest debugging value per line in the whole plan, and they
   need only O1.2.
5. **O3, O4.3–O4.5, O5** — in any order.

# Risks

- **O0.3 is the only real perf risk.** `lint()` re-lays out every text node in
  its tofu path (`app.rs:1846`). Measure on `benches-competitive` and land the
  number in the commit message; fall back to an every-Nth-frame cadence for the
  expensive checks if a debug frame regresses more than ~2×.
- **O2.2 changes existing behaviour.** Fixing the W0103 edge test will surface
  findings in apps that are silent today, including the example crates. Treat
  each as real until proven otherwise; budget for golden churn.
- **O2.3 is O(n²) if written naively.** Bound it, and `log()` the bound when it
  trips — a silent cap reads as "covered everything" when it didn't.
- **Ring pressure.** 1000 entries (`state.rs:507`). Every site in this plan is
  edge-triggered for that reason; a single unconditional per-frame line defeats
  the whole mechanism. O0.2 exists so nobody has to remember.
- **Two of these findings are corrections to earlier claims**, kept explicit so
  the record stays honest: `get_styles` returns the *computed cascade* result,
  not the raw declaration (O3.2 is narrower than first stated), and `ScrollInfo`
  already carries `max_x`/`max_y`, so "content below the fold" was never a gap
  (O2.2).
