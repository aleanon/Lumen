# Framework review against the project goals — 2026-07-06

Scope: the whole framework as of `main` (4aedd82), reviewed against the three
stated goals: **(1) peak performance, (2) minimal resource usage for that
performance, (3) an agent's ability to verify applications in dev mode.**
Method: three parallel deep-dive code reviews (one per goal) plus live
measurements on this box (RTX 4070, X11, release builds): criterion benches,
`scripts/perf_gate.sh` budgets, `scripts/size_gate.sh`, idle CPU/RSS sampling of
a running window, and a full end-to-end drive of the live agent endpoint
(`just run-agent accordion` → getTree → selector click → state verify →
screenshot → lint).

## Scorecard

| Goal | Verdict | One-line summary |
|---|---|---|
| 1. Peak performance | **Strong, with one architectural ceiling** | Idle and scrolling are best-in-class; any *structural* change rebuilds O(tree), and the GPU path is non-incremental. |
| 2. Minimal resource usage | **Excellent at idle; two real costs** | ~0.1–0.5% CPU idle, event-driven; but 22 MB binaries (font-dominated) and per-interaction rebuild churn. |
| 3. Agent verifiability | **The differentiator is real — proven live** | Full observe→act→verify loop works today; the live-window leg lacks auto-wait/logs and the spec over-promises. |

## Measured numbers (this session)

All `perf_gate.sh` budgets **pass with large margins**:

| bench | measured | budget |
|---|---|---|
| idle_frame | 26 ns | 2 ms |
| signal_update_large_vec (100k Vec) | 16.5 ns | — |
| layout_10k_dirty_subtree | 0.42 ms | 2 ms |
| vlist_1m_scroll (1M rows) | 0.71 ms | 8.33 ms |
| data_grid_1m_scroll (1M rows) | 1.70 ms | 8.33 ms |
| cull_100k | 0.54 ms | 5 ms |
| scope_memo_one_of_many | 0.68 ms (−89% since F1) | — |
| text_list_changed_frame | **7.03 ms** | — |

Live window (accordion, wgpu direct-to-surface): idle CPU **0.1–0.5% of one
core** over 10–15 s; RSS 205–233 MB of which only **~40 MB is anonymous heap**
(the rest is the 32 MB binary incl. font + NVIDIA/Vulkan file-backed mappings);
38 threads (32 of them an eagerly-spawned task pool). `hello` release binary:
**22.1 MB** vs the <5 MB target — 15.5 MB is the embedded GoNotoKurrent font.

---

## Goal 1 — Peak performance

### What is genuinely fast (verified in code and benches)

- **Idle is best-in-class.** `about_to_wait` picks `ControlFlow::Wait` when no
  deadline exists (`lumen-shell/src/lib.rs:501–521`); an idle pump is 26 ns
  (write-gen + `structural_reads.is_current` skip, `lumen-widgets/src/app.rs:505–516`);
  the shell presents nothing on `Damage::None`. This beats egui (re-runs the UI
  fn per repaint) and matches or beats iced.
- **The reactive core is done right.** `Signal::update` mutates in place
  (16.5 ns on a 100k Vec); F1 `cx.scope` memoization returns cached subtrees
  when recorded signal versions are current (`element.rs:516–560`) — the
  one-of-200-rows bench dropped 89% when it landed.
- **Paint-only patches** (F3.4): background-binding changes patch one node and
  repaint without rebuild or relayout (app.rs:1281–1306).
- **Text is heavily cached**: parley shape cache, origin-relative glyph-run
  cache (R5 slice, ~50× DL-emission win), per-glyph swash raster cache, and a
  shelf-packed GPU glyph atlas with instanced quads. Zero-copy font blob.
- **Virtualization works as designed**: 1M-row list/grid scroll cost is
  independent of row count (0.71/1.7 ms including pump).
- **Damage bookkeeping with a byte-identical contract** (R2), retained display
  list, `damage_between` diff, and a deterministic CPU golden renderer gating
  the GPU backend (R0 corpus at 1× and 2×).
- **Rendering plan status correction**: R0–R3 plus the R5 run-cache slice are
  implemented, including direct-to-surface present (one device, no
  GPU→CPU→GPU readback). Only R4 (threaded layout, blocked by taffy's serial
  solve) and full R5 fragment splicing remain.

### The performance ceiling (ranked liabilities)

1. **O(tree) reconstruction on every structural change (HIGH).** The F2
   retained node graph was descoped, so any structural change rebuilds
   everything downstream: new `Tree` + new `TaffyTree` + `meta` map
   (app.rs:1378–1390), full taffy solve, full `compute_styles` (per-node
   selector matching with fresh `String`s, app.rs:1434–1483), full semantics
   tree (app.rs:2385–2429), full dep-index (app.rs:1411–1429). Changed-frame
   cost scales with tree size, not change size — egui-class, not Xilem-class.
   Memoized scopes also deep-clone their cached `Element` subtree per rebuild
   (element.rs:556).
2. **Hover wipes all view caches (HIGH, compounding #1).** Pointer motion over
   a clickable node sets `visual_changed`, forcing a rebuild *and* calling
   `clear_view_caches()` (app.rs:523–525) — mouse movement over a large UI
   produces fully unmemoized O(tree) rebuilds per event.
3. **CPU changed-frame raster is full-frame (HIGH on the CPU path).**
   `render_damage` deliberately renders the whole frame and crops (tiny-skia AA
   is not translation-invariant, `cpu.rs:52–75`) — so damage saves compositing,
   not raster: `text_list_changed_frame` = 7 ms at only 500 nodes/400×400.
   This is the cost CI and GPU-less machines pay.
4. **GPU path is non-incremental (MEDIUM).** Damage is computed then ignored
   (app.rs:2314–2317, no scissor); `DrawCmd::Path` is re-tessellated by lyon
   every frame (gpu.rs:2295–2348 — the R1.3 tessellation cache was never
   built); uniform buffers/bind groups are created per frame; images and
   gradient ramps are re-uploaded as new textures every rendered frame; plus a
   fullscreen root-texture→swapchain blit each frame. Fine on an RTX 4070;
   a real battery/perf cost on mobile and path-heavy scenes.
5. **No viewport culling in the paint path (MEDIUM).** `cull_visible` is
   threaded and benched — and used only by the bench. Offscreen
   non-virtualized content is emitted, diffed, and rasterized.
6. **Whole-cache eviction cliffs (LOW).** Shape/run/glyph/shadow caches
   `clear()` entirely at cap — crossing SHAPE_CACHE_CAP=2048 re-shapes
   everything at once (a hitch) instead of LRU-evicting.
7. **Single-rect damage union (LOW-MED, CPU path).** Two small distant changes
   damage the whole span between them; a changed layer forces `Damage::Full`.

### Recommendations (priority order)

1. **Revive F2 / finish the retained pipeline** — retain `Tree`/`meta` across
   pumps, splice only re-run scopes, stop rebuilding semantics + dep-index for
   untouched subtrees. This is most of a changed frame once raster is GPU-side,
   and it fixes #1 and (with hover keyed into the reactive graph) #2.
2. **Stop hover from clearing scope memos** — key hover/focus/pressed into the
   dependency graph or scope the invalidation to affected subtrees. Small
   change, large win; independent of full F2.
3. **CPU path: cull draw commands against the dirty rect** (and/or clip-masked
   full-size raster) so damage saves raster, not just compositing; wire the
   existing `cull_visible` into paint.
4. **GPU: tessellation cache (path+style hash), persistent uniform/instance
   buffers, texture cache keyed by ImageId, damage scissor, skip the root blit
   when no backdrop layers.** All incremental, all planned-but-unbuilt.
5. **LRU/half-eviction instead of clear-all** in text/shadow caches.
6. **Cheapen memo hits**: return cached subtrees as `Rc<Element>`/COW, intern
   scope/signal string keys.

---

## Goal 2 — Minimal resource usage

### The good news

- **Idle behavior — the most important GUI resource property — is exemplary
  and empirically confirmed**: event-driven `ControlFlow::Wait`, redraws only
  on input/resize/wake/deadline, present skipped when nothing painted, springs
  stop requesting frames when settled, no caret-blink timer, vsync-capped
  `Poll` during animation only. Measured ~0.1–0.5% of one core sitting idle.
- **No tokio** — a hand-rolled mpsc thread pool; `serde_json` is droppable via
  `--no-default-features` (snapshot feature); agent RPC is compile-time
  opt-in; parley has system-font enumeration off. Dependency discipline is
  real.
- Real heap is modest (~40 MB anon RSS for a small app); caches are
  entry-bounded; scope-memo GC exists (F5).

### The liabilities (ranked)

1. **Binary size: 22.1 MB vs the <5 MB target.** 15.5 MB is
   `GoNotoKurrent-Regular.ttf` via `include_bytes!` (`lumen-text/src/lib.rs:25`);
   windowed examples ≈ 32.6 MB. The release profile has `lto = "thin"`,
   `codegen-units = 1` but **no `strip`, no `panic = "abort"`** (stripping
   alone saves little — the font dominates). Fix: subset a Latin+symbols
   default face (~1–2 MB), make pan-Unicode opt-in (feature or
   `App::with_font`), add `strip = true` + `panic = "abort"`. Note the irony:
   despite shipping a 15.5 MB pan-Unicode font, `▼`/`▶` render as tofu.
2. **Per-interaction rebuild churn** — same F2/hover items as Goal 1 (they are
   the resource story too: every hover move allocates a full tree, taffy tree,
   meta map, semantics tree, dep-index).
3. **Per-frame GPU re-uploads during animation** — images and gradient ramps
   re-uploaded, buffers re-created each rendered frame (idle scenes don't pay).
4. **Eager 32-thread task pool** (`lumen-core/src/tasks.rs:238–243`) — zero
   idle CPU (blocked on recv) but 32 stacks on a 32-core box for apps that may
   never spawn a task. Lazy-spawn or cap at min(cores, 4).
5. **Byte-unbounded image-valued caches** — 64 shadow sprites of full
   `RgbaImage`s can be tens of MB; clear-all eviction hitches (same as Goal 1
   #6). CPU path allocates one full-window Pixmap per `PushLayer` per paint.

---

## Goal 3 — Agent verifiability in dev mode

### Proven live this session

The full loop was driven end-to-end against a real window
(`just run-agent accordion`, port ready in ~6 s warm):

1. `ui.getTree` → 3.8 KB semantic tree with roles, labels, pixel bounds,
   available actions, states (`expanded`/`collapsed`), style classes.
2. `input.click {selector: 'button:text-contains("return policy")'}` → ok.
3. Re-query → state flipped to `expanded`. **Round-trip verification works.**
4. `ui.screenshot` → real 520×620 PNG of the live scene; element-zoom crop
   with box/ink overlay works; `ui.lint`/`app.diagnostics` respond.

The agent RPC dispatches against the **same live runtime** driving the window
(`lumen-shell/src/lib.rs:239–253`) and requests a redraw after each action —
agent state and window state cannot diverge.

### Why this is genuinely differentiated (vs Playwright as the benchmark)

- **One tree feeds a11y + test locators + agent** (ADR-009) — selectors cannot
  drift from what is rendered, unlike DOM/ARIA projections.
- **Reactive introspection is novel**: `ui.getDeps`, `ui.whatDependsOn`,
  `ui.lastChange` (idle/patch/rebuild) let an agent predict and confirm *why*
  the UI updated. No web equivalent.
- **Structured, queryable lint** (overflow W0103, ink-clipping W0104, zero-area
  W0105, unnamed-focusable W0301, WCAG contrast/name) + `auto_repair()`
  detect→fix→verify loop, proven in agent-gauntlet-2 with zero human edits.
- **Determinism beats web**: CPU reference renderer with bit-exact goldens,
  virtual clock, one synthesized-input path shared by tests/agent/OS, JSONL
  traces with embedded failure screenshots, and `session.exportTest` that emits
  a runnable `cargo test` from a recorded exploration (stronger than Playwright
  codegen, which records actions but not assertions).
- Four gauntlet examples gate this in CI (agent-gauntlet, -2, -media, inspector
  self-drive).

### Gaps, ranked by impact on verifying a real app

1. **Spec↔implementation drift (highest risk — agents read the spec).**
   `.ai_docs/03-spec-semantics-agent.md` documents ~15 methods/params that do
   not exist in `dispatch` (`app.logs`, `state.get`, `events.subscribe`,
   `input.drag`, `ui.waitFor`-style `timeout_ms`, `input.type {clear}`,
   screenshot `max_width`, bearer auth, …). An agent following the spec fails.
2. **Live actions do not auto-wait.** `resolve_action` pumps exactly once
   (lumen-agent/src/lib.rs:510–516); the 5 s auto-wait exists only headless on
   the virtual clock. Against async/animated live apps the agent must busy-poll
   `ui.getTree`. Biggest flake source.
3. **No log/console access over the protocol** — panics and `println!` are
   invisible unless the agent scrapes the process stderr.
4. **`app.perf` is stubbed** (hardcoded zeros, lib.rs:376–379) even though
   `pump()` returns `FrameStats` — perf assertions via the protocol are
   impossible; gauntlet-media wall-clocks around it.
5. **Session record→export is unavailable live**: the shell calls plain
   `dispatch`, not `Session::dispatch` (shell lib.rs:243) — the flagship
   "explore live, commit a regression test" story only works headless.
6. **Selector/id mismatch (found live)**: `ui.getTree` returns `node-13`-style
   ids, but the selector grammar rejects them — the agent must re-derive a
   role/text selector; the error is a raw `NotFound {{ nearest: [] }}` debug
   string with no suggestions despite the nearest-miss machinery existing.
7. **Input holes**: no drag between nodes over the protocol, no hover method,
   no right/double click, vertical-only scroll, no IME composition, `type`
   can't clear, ~14 named keys.
8. **Pixel verification limits**: goldens are CPU-renderer only (the GPU path
   users see has no golden/perceptual diff, and since the linear-light switch
   they intentionally diverge); no `.diff.png`; tofu glyphs mean screenshots
   can't confirm iconography — and `ui.lint` does **not** flag missing glyphs
   (found live: semantics said `▼`, pixels showed tofu, lint was clean).
9. **Lifecycle ergonomics**: fixed default port, readiness only by polling,
   teardown via `pkill`, no `app.quit`, no packaged client (every agent
   re-implements the socket client; `mcp_manifest()` is a static list with no
   server behind it).
10. **`lumen-test` assertion semantics mislead**: docs say auto-retrying, but
    only `to_have_text` retries; the rest fail immediately with `Timeout`.

### Recommendations (priority order)

1. Reconcile `.ai_docs/03` with the implementation (implement or mark
   "planned") — cheapest, highest-leverage fix for agent success rate.
2. Auto-wait in `resolve_action` + a `ui.waitFor {selector, state?, text?,
   timeout_ms}` method.
3. `app.logs` (ring buffer) and real `app.perf` from `FrameStats` — data
   already exists.
4. Route the shell through `Session` so `session.exportTest` works live
   (near one-line at shell lib.rs:243).
5. Accept `node-N` ids as selectors, and render `NotFound` errors with the
   nearest-miss suggestions.
6. Port 0 + address discovery file, `app.quit`, a packaged client
   (`lumen-cli agent call …`) and a thin MCP server over it.
7. Make `ui.lint` detect tofu (glyph-not-found) — the one live defect this
   session that only pixels caught.
8. `.diff.png` on golden mismatch + perceptual GPU compare (spec 05 §4).

---

## Overall assessment

The framework's stated goals are in the right order of health inverted: the
**agent-verification story (goal 3) is the strongest and genuinely novel** —
proven end-to-end live this session — but its live-window leg needs the
auto-wait/logs/session fixes to be reliable for real apps, and the spec must
stop promising methods that don't exist. **Resource usage (goal 2) is
exemplary where it matters most** (idle), with binary size being a solved
problem the moment the font is subset. **Peak performance (goal 1) has an
outstanding idle/scroll/text story and one honest architectural ceiling**: the
O(tree) rebuild behind every structural change (descoped F2) — compounded by
hover wiping the scope-memo cache — and a GPU backend that redraws everything
every frame. Both ceilings are already correctly diagnosed in the project's own
plans; the plans just haven't been executed to the end.

If only three things get done next: **(1) F2 retained pipeline (or at minimum
stop hover from clearing view caches), (2) live-agent auto-wait + spec
reconciliation, (3) font subsetting + `strip`/`panic=abort`.** Together they
close the largest gap in each goal.
