# Docs ↔ Code implementation diff — 2026-07-08

> **HISTORICAL RECORD — RESOLVED (D9.3, 2026-07-20).** Every liability in
> this review was worked off by `docs/plan-remediation-2026-07.md`
> (phases D0→S0→A→B→C→T→W→R→P→M→E→D9), executed 2026-07-09 → 2026-07-20,
> commit range 7580e74..8a042c2. Per-item resolution evidence lives in
> `.ai_docs/07-decision-log.md` (the "Remediation 2026-07" section, one
> entry per landed task) and the flipped marks in `.ai_docs/06-task-graph.md`.
> The v2 scorecard proving the goal-level liabilities closed is
> `docs/review-goals-2026-07-v2.md`.


Three-way diff of **(a) what the docs claim is implemented** (task-graph ☑s,
plan "done" markers, module doc comments), **(b) what the normative specs say
should be implemented** (.ai_docs/01–05), and **(c) what the code actually
contains** — ending in a consolidated list of everything left to implement.

Method: four parallel code audits (core spec 02+01, styling spec 04, tooling/
platforms/milestones 06+03§4+05§1/6, agent protocol 03§3 + testing 05 — the
last verified in the 2026-07-06 goals review and re-used here). Every verdict
below was checked against source with file:line evidence; this document keeps
the key evidence and the conclusions.

## 1. The headline pattern

**`.ai_docs/06-task-graph.md` marks every task ☑ through M7, but for M5–M7
(and parts of M0/M2/M3/M4) the code implements a headless deterministic
*model* of each feature while the OS/hardware/CI half does not exist.**
`docs/backlog.md` and `docs/cross-platform-readiness.md` say this honestly —
they are the accurate documents. The task-graph checkboxes (and a handful of
module doc comments) are the doc bug. The normative specs 02–05 describe a
system that is roughly 60–70 % implemented: the semantic/agent/test/render
core is real and strong; the styling *runtime*, dev-server protocol, platform
shells, and ecosystem layers fall well short of their spec text.

### Direct ☑-vs-reality contradictions (checkbox wrong, backlog right)

| Task-graph ☑ | Reality | Honest doc |
|---|---|---|
| T0.9 "`#[lumen::test]` macro" | No attribute macro exists; tests hand-construct `TestApp` + `block_on` (lumen-macros has only `stable_handler!`, `text!`) | — |
| T2.3 tier-2 hot patch | Swap mechanics real + tested (fixtures hot_a/b/c, libloading, abi-hash gate) but **no live orchestration** — no incremental-rebuild driver, no push into a running app | backlog (partial) |
| T2.4 tier-3 snapshot restart | State restore real (`AppSnapshot`, `run_headless_restored`); the "kill/rebuild/relaunch" is an in-process drop in a test — no process-level driver | — |
| T3.1 Android touch/IME | `lumen-shell-android/src/lib.rs:2-3` claims touch is fed into the input queue; `imp.rs:65` **drops all input events** (`_ => {}`). No IME, safe areas, back button | cross-platform-readiness |
| T3.3/T3.4 iOS shell | `render_into()` only; Obj-C template uses CoreGraphics (not Metal) and references FFI symbols (`lumen_ios_touch`/`lumen_ios_text`) that **don't exist** | cross-platform-readiness |
| T3.x "in CI" | `.github/workflows/mobile.yml` is 100 % commented out | — |
| T5.1 web target | One-shot CPU `render_into` + 2D-canvas `putImageData`; no WebGPU/WebGL2, no wasm event loop, agent bridge is a comment; "headless Chromium" leg absent; size script prints, doesn't gate | cross-platform-readiness |
| T5.2 desktop OS integration | Zero OS deps (no arboard/rfd/muda/tray); shell single-window; acceptance tests assert on in-memory structs | backlog A4 (sandbox-blocked) |
| T4.3 AccessKit / T7.4 screen readers | Role/state map + `TreeUpdate` builder real (a11y.rs) but **no `accesskit_winit` adapter** — the tree never reaches the OS; no AT automation anywhere | backlog A5; a11y-checklist:58 |
| T6.1 Vello-class rasterizer | `Backend::VelloCompute` is a bare enum variant whose own doc says "integration is future work"; the GPU path is wgpu + lyon CPU tessellation | — |
| T6.2 media codecs | PNG only (tiny-skia); no jpeg/webp/avif/GIF/APNG/Lottie anywhere; SVG is a ~150-line subset parser | backlog B3 (still to-do) |
| T6.3 audio/video/capture | Stub models only: procedural `TestPattern` video, `AudioBuffer::sine()`, empty `CaptureSource` enum; no cpal/decoder/camera deps | backlog D2 |
| T6.4 motion | Springs + `SharedElement` morph + `Timeline` choreography exist as library code with tests; **not wired** into routes/gestures; no keyframe evaluator | backlog D1 |
| T6.5 advanced text | `RichDoc` = bold/italic runs + find/replace + cross-selection; its own header admits lists/tables/links/images/spell-check/variable-axes/CRDT "layer on this" (i.e. absent). `rich_text_editor` widget is append-only markdown-lite and **doesn't use RichDoc** | — |
| T6.6 perf at scale | perf_gate real (5 budgets, in CI). Multi-threaded layout **parked** (backlog R4); no memory profiler/leak gate; no cold-start measurement; size_gate prints without failing and isn't in CI | backlog R4 |
| T7.1 distribution | `lumen package` → one unsigned `.bundle/` dir + manifest; apk via script; no msix/dmg/AppImage/ipa/signing/notarization/auto-update/SBOM | backlog C3 |
| T7.2 plugin ecosystem | `lumen add` appends `crate = "*"` to Cargo.toml; third-party widgets are a **source-level** `LeafWidget` trait; no stable ABI, no registry | — |
| T7.3 hardening | Panic containment real (boundary.rs, E0701). **No fuzz targets** (proptest in exactly one file, tree only), no crash-report hook, no telemetry | — |

## 2. Spec 02 (core) vs code

Solidly implemented as specced: Event enum + capture/bubble dispatch (every
variant incl. Gesture/ImePreedit/Timer/Custom; events.rs:218-254, 332-360),
DrawCmd all 7 variants + conic gradients + blend modes (display_list.rs:237-300),
SoA hot data + array hit-test (tree.rs:58-68, 253-262), generational NodeIndex,
Headless API, diagnostics registry file, snapshot-feature State bound.

Drift:

| Claim | Verdict | Detail |
|---|---|---|
| §3 `trait Widget` (build/layout/paint/event/semantics) | **CONTRADICTED** | Only `LeafWidget { measure, paint, semantics }` (element.rs:79-86). No `build()`/`event()`/`type_name()`; custom leaves get **no event hook**. element.rs:3 still says "full Widget trait arrives in T0.10" — it never did |
| §3 `#[component]` + PartialEq-props memo | MISSING | lumen-macros has only `stable_handler!`/`text!`; memoization is `cx.scope` signal-read-based instead |
| §3 ElementBuilder `.key()` / generic `.on()` / typed `.style()` | PARTIAL | `.id`/`.class` exist; `.style` takes `LayoutStyle` not the 04 §8 typed `Style`; no `.key()` (helper `widgets::keyed()` instead); fixed `on_click/on_key/...` hooks instead of `.on(EventKind, h)` |
| §4 `cx.memo`/`cx.effect` | PARTIAL | Exist on `Runtime` only (state.rs:508,490), reachable via `cx.runtime()`; not on BuildCx |
| §4 `#[state_registry]` | MISSING | No typetag-style mechanism anywhere |
| §4 `Checkpoint` trait | PARTIAL | Ad-hoc `Runtime::snapshot/load_pending/finish_restore` + `is_quiescent`; no trait, no quiesce/resume verbs |
| §8 `App::run(self) -> !` | PARTIAL | `RunExt::run(self, size)` returning `()` via lumen-shell |
| §9 diagnostics | PARTIAL | Emitted: W0002, E0101, E0102, E0104, W0103/4/5, E0201, W0401, E0701. **Defined but never emitted: W0001 (dup id), W0301 (unnamed focusable — the audit lint checks it another way), E0103 (style type mismatch)** |
| §10 widget set | PARTIAL | M0 10/10 ✅, M1 17/20, M2 4/10, M3 6/6 ✅, M4 5/11. Missing everywhere: **Popover, Sheet, Drawer, SearchField, Combobox, ColorPicker, Skeleton, Avatar, Pagination, RangeSlider, FilePicker, pie chart**; example-only: Toast, Spinner, Chip, line chart; Align is a modifier not a widget |
| §11 facade discipline | **CONTRADICTED** | 91 of 97 example src files import `lumen_core::`/`lumen_widgets::`/`lumen_render::` directly; 38 example Cargo.tomls depend on internal crates; only 5 use the `lumen` facade |
| 01 §3 "incremental layout: dirty subtrees only" | **CONTRADICTED** | Live pump does `LayoutTree::new()` + full `compute()` every rebuild (app.rs:1379,1392); `relayout_subtree`'s only caller is a lumen-layout test |
| 01 §2 GPU "layer caching" | PARTIAL | Damage/partial redraw real on CPU path; layers are per-frame passes; no cross-frame layer texture cache; GPU path ignores damage |
| 01 §6 shaders | PARTIAL | WGSL fragment-on-rect + hot reload + E0201 + CPU fallback real; missing pointer/theme built-ins, declared typed uniforms (fixed `params: vec4` only), full custom pipelines |
| 01 §9 cold start <300 ms | MISSING | No measurement or gate anywhere (binary <5 MB also missed: 22.1 MB, font-dominated) |

## 3. Spec 04 (.lss styling) vs code — the biggest runtime gap

**The parser knows all ~70 v1 property names; `apply()` maps ~16 to typed
fields; paint consumes fewer still.** Key verdicts (evidence in lumen-style):

| Area | Verdict | Detail |
|---|---|---|
| Layout properties from `.lss` | **MISSING at runtime** | `compute_styles` runs **after** `layout.compute` (app.rs:1392→1402) — no `.lss` layout property (display/flex-*/grid/width/padding/…) affects layout at all. Grid track lists aren't even parsed |
| Nested rules `&:hover` | **Parsed, dropped** | `resolve()` ignores `rule.nested` (lib.rs:130,242); `& > .part` is a parse error |
| `@media` at runtime | **CONTRADICTED** | Engine (`eval_query`/`resolve_media`) correct and tested, but runtime calls plain `resolve()` which flattens media rules in **unconditionally** — they always apply regardless of window size/platform. `@media container(...)` is a parse error; no `.container()` API |
| `transition:`/`animation:` | **Unwired** | Parsed as names only; nothing connects them to the Scheduler. No keyframe playback exists (AST only). Scheduler (anim.rs) does transitions/springs and is exercised only by tests |
| Theme switch 150 ms animation | MISSING | `set_theme` rebuilds synchronously |
| Relative colors `oklch(from $x calc(…) …)` | MISSING | Numeric `oklch()` only; `+` isn't a lexer token |
| Widget parts (`slider .track`, `cx.part()`) | MISSING | No part classes on built-ins, no `part` API |
| Cascade origins | PARTIAL | Only `Origin::App` is ever constructed — framework defaults are hardcoded paint fallbacks, inline `.style()` bypasses the cascade, `theme`/`inline`/`default` sources unreachable in `get_styles` |
| State selectors | PARTIAL | Runtime exposes only `"focused"`/`"hovered"` — and spec examples write `:hover`, which won't match `hovered`. No disabled/pressed/checked |
| Visual/typography properties | Mostly parse-only | Applied+rendered: background color, border (shorthand), uniform border-radius, backdrop-filter (incl. beyond-spec refraction/specular), text color. Computed-but-ignored: opacity, font-size/weight (deferred per comment). Parse-only: gradients-in-background, shadow, blend-mode, filter, clip, transform, z-index, visibility, cursor, font-family/style/features/variation, line-height, letter-spacing, text-align/overflow/wrap/decoration, selection-color |
| `style_parity!` | PARTIAL | Exists as a local test macro over 11 hand-picked properties, not a set-equality assertion; `Style` has ~12 setters vs ~70 spec properties |
| Errors | PARTIAL | E0101/E0102 (did-you-mean)/E0104 + atomic reject + spans ✅. **E0103 never emitted** (type mismatches silently ignored). `border-width`/`border-color` are applied but missing from KNOWN_PROPERTIES → spurious E0102. Unknown units silently become unitless |
| `get_styles` serialization | PARTIAL | `{value, source}` canonical forms ✅; `span` field missing; only `"stylesheet"` source reachable |

## 4. Spec 03 (agent protocol) + 05 (testing) vs code

(Verified in the 2026-07-06 review; summarized for completeness.)

**03 §3 methods in spec but absent in code:** `app.logs`, `state.get`,
`events.subscribe`, `input.drag`, `input.gesture`, `app.setValue`,
`app.command` (+ `cx.register_command`), `session.start/stop`, `reload.apply`;
params `timeout_ms` (no auto-wait — `resolve_action` pumps once), click
`pos/button/count`, type `clear`, scroll `to`, getTree `selector`, screenshot
`max_width`; bearer-token auth. `app.perf` is a stub (hardcoded zeros).
Transport drift: spec says WebSocket served by the CLI dev server; reality is
raw TCP newline-JSON served inside the shell (`LUMEN_AGENT_ADDR`), plus a
WebSocket path used only by tests. `mcp_manifest()` is a static list; no MCP
server. The shell calls `dispatch`, not `Session::dispatch`, so
`session.exportTest` is unavailable against a live window.

**Reverse drift (code richer than spec — document these):** `ui.getDeps`,
`ui.whatDependsOn`, `ui.lastChange`, `ui.lint`, `ui.probe`/`probeRegion`,
`ui.getMenu`/`menu.invoke`, `app.systemRequests`, `ui.getWindows`,
`input.setLocale`, `input.invokeAction`, `input.drop`, `clipboard.read/write`,
element-zoom screenshots `{selector, scale, overlay}`.

**05 vs lumen-test:** no `#[lumen::test]` macro (T0.9 ☑ contradicted); no
`scale`/`platform` test options; Locator lacks `right_click`, `type_text`,
`scroll_into_view`; `expect` lacks `to_be_visible`; no `TestApp::run_command`;
only `to_have_text` auto-retries (others one-shot but report "Timeout"); golden
mismatch writes `.actual.png` but **no `.diff.png`**; **no GPU perceptual
goldens** (ΔE spec unimplemented — CPU exact only; GPU and CPU deliberately
diverge since the linear-light switch); harness bundles one GoNotoKurrent face,
not the specced Noto Sans/CJK/Color-Emoji set (decorative glyphs render tofu).
`lumen test --platform gpu` missing (CLI accepts android|ios_sim|web only).

**03 §4 dev-server wire protocol: effectively absent.** `proto.rs` is a stub
enum (StyleUpdate/Ping shapes, never sent anywhere); no length-prefixed
transport, no `LUMEN_DEV_ADDR` client, no `shader_update`/`asset_update`/
`dylib_update`/`restart_request`/`hello{abi_hash}`/`state_snapshot`/`pong`.
What exists instead: in-process `notify` watchers (shell `LUMEN_WATCH_LSS`,
CLI `watch_file`/`tier1_reload`).

**01 §8 CLI:** subcommands are `new/run/test/package/add` (with `--json` ✅);
`inspect` and `agent serve` (both listed in 01 §8) are missing; `add` is
undocumented in the spec.

## 5. Consolidated: LEFT TO IMPLEMENT

Deduplicated across all audits; grouped by subsystem. **Bold** = also a
top-priority item in the goals review (docs/review-goals-2026-07.md).

### Core runtime
1. **Incremental layout in the live pump** — wire `relayout_subtree` in
   instead of `LayoutTree::new()` + full compute per rebuild (pairs with the
   F2 retained-graph revival).
2. `trait Widget` unified archetype per 02 §3 (or amend the spec to bless
   `LeafWidget` + composite fns) — incl. an **event hook for custom leaves**.
3. `#[component]` attribute macro with PartialEq-props memoization.
4. ElementBuilder: `.key()`, generic `.on(EventKind, h)`, `.style()` taking
   the typed `Style`.
5. `cx.memo`/`cx.effect` on BuildCx (thin forwarding to Runtime).
6. `Checkpoint` trait (quiesce/serialize/restore/resume) formalizing the
   existing snapshot fns; `#[state_registry]` for stored trait objects.
7. Emit the three dead diagnostics: W0001 (duplicate StableId), W0301
   (unnamed focusable — audit lint covers it, the diagnostic doesn't), E0103
   (style type mismatch).
8. `App::run(self) -> !` shape (or amend spec to the `RunExt` reality).
9. Facade discipline: migrate 91 example files / 38 Cargo.tomls onto `lumen`.
10. Missing widgets: Popover, Sheet, Drawer, SearchField, Combobox,
    ColorPicker, Skeleton, Avatar, Pagination, RangeSlider, FilePicker,
    pie chart; promote Toast/Spinner/Chip/line-chart from examples; Align.

### Styling runtime (largest single cluster)
11. **Apply `.lss` layout properties to layout** (compute styles before
    `layout.compute`; map flex/grid/min-max/aspect/position/inset/overflow/
    per-side padding-margin; parse grid track lists).
12. Apply nested `&` rules; support `& > .part`.
13. Gate `@media` on a live `MediaContext` (today: always applies);
    `@media container(...)` + `.container()`.
14. Wire `transition:`/`animation:` to the Scheduler; keyframe evaluator.
15. Apply the parse-only visual/typography properties (gradients, shadow,
    opacity, transform, filter, blend-mode, z-index, visibility, cursor,
    font-family/…​, line-height, letter-spacing, text-align/overflow/wrap/
    decoration, selection-color); font-size/weight → text measurement.
16. Cascade origins beyond App (default sheet, theme origin, typed inline);
    full state vocabulary (`:hover` alias, disabled/pressed/checked);
    only-affected-node restyle.
17. Theme-switch 150 ms color animation; OS reduced-motion signal;
    relative colors `oklch(from … calc(…))`; `rgb()` alpha.
18. Widget parts (`.track`/`.thumb` + `cx.part()`); full `Style` setter
    parity + a real set-equality parity test; fix KNOWN_PROPERTIES
    (border-width/color); E0103 emission; span in `get_styles`.

### Agent protocol & dev loop
19. **Auto-wait on live actions** + `ui.waitFor`; `timeout_ms` per 05 §3.
20. `app.logs` (ring buffer) and real `app.perf` from FrameStats.
21. Missing methods: `state.get`, `events.subscribe`, `input.drag`,
    `input.gesture`, `app.setValue`, `app.command`, `session.start/stop`,
    `reload.apply`; param gaps (click pos/button/count, type clear, scroll to,
    getTree selector, screenshot max_width); bearer auth for non-loopback.
22. Route the shell through `Session` so live `session.exportTest` works.
23. Real MCP server over the protocol; packaged client
    (`lumen agent call …`); accept `node-N` ids as selectors; render
    NotFound errors with the nearest-miss data.
24. The 03 §4 dev-server wire protocol (or officially replace it with the
    in-process watcher design and rewrite §4): transport, `LUMEN_DEV_ADDR`,
    dylib_update/restart_request/state_snapshot legs.
25. Live tier-2 orchestration (incremental rebuild + push to a running app);
    tier-3 process-level restart driver.
26. CLI: `inspect`, `agent serve`, `test --platform gpu`.

### Test harness
27. `#[lumen::test]` macro (+ scale/platform options).
28. Locator: right_click, type_text, scroll_into_view; expect: to_be_visible;
    make all `expect` assertions actually retry; `TestApp::run_command`.
29. Golden `.diff.png`; GPU perceptual compare (ΔE Oklab) per 05 §4.
30. Bundle the specced font set (or subset + document the single-font
    reality); tofu (missing-glyph) detection in `ui.lint`.

### Platform shells
31. Android: touch → input queue (currently dropped), IME/soft keyboard,
    safe areas, back button; fix the lib.rs doc claim.
32. iOS: real shell (Metal present, the referenced-but-missing
    `lumen_ios_touch`/`lumen_ios_text` FFI), Xcode project generation.
33. Web: wasm event-loop shell, WebGPU + WebGL2 present path, agent bridge
    (WebSocket/postMessage), headless-Chromium test leg, enforced wasm size
    gate.
34. Desktop OS integration (backlog A4, ADR-003 escalations): arboard
    clipboard first, then rfd dialogs, muda menus, tray, notifications, OS
    drag-and-drop, multi-window/multi-monitor.
35. AccessKit platform bridge (`accesskit_winit` publishing the existing
    TreeUpdate, actions routed to `inject()`); later: scripted AT smoke tests.
36. Re-enable mobile CI (`mobile.yml` fully commented out); put GPU parity,
    size gate, and golden suites into CI.

### Rendering & performance
37. **F2 retained node graph** (with #1) — the O(tree) rebuild ceiling; stop
    hover from clearing scope memos.
38. GPU damage scissor; lyon tessellation cache; persistent uniform/instance
    buffers; texture cache for images/ramps; cross-frame layer caching.
39. R4 multi-threaded layout (parked; needs the two-phase driver) or
    officially rescope T6.6.
40. Memory profiler + leak gate; cold-start measurement + gate; make
    size_gate fail; **font subsetting** to make <5 MB reachable.
41. Vello-class compute rasterizer behind `Backend::VelloCompute` (currently
    a placeholder) or drop the T6.1 claim.

### Media, motion, text
42. Image codecs: jpeg/webp/avif/GIF/APNG (ADR); full SVG (gradients,
    transforms, text, clips); Lottie.
43. Audio playback, video decode, camera/mic capture (only deterministic
    stubs exist).
44. Motion wiring: route-level shared-element transitions, gesture bindings;
    (models already exist in lumen-style/motion.rs).
45. Rich text: lists/tables/links/images in RichDoc, spell-check hooks,
    variable-font axes, CRDT hooks; make `rich_text_editor` use RichDoc with
    real caret/selection editing.
46. Examples-plan PENDING tail (08 §4): HTTP client on `resource()`
    (pokedex/download_progress), QR encoder, vectorial text, animated images,
    OS binding examples (exit/url_handler/multi-window/integration), full
    sysinfo. (Anchored dropdowns landed since — pick_list.rs.)

### Ecosystem & production
47. Installers (msix/dmg/AppImage/ipa), signing/notarization, auto-update,
    SBOM/supply-chain gates ( `lumen package` currently emits one unsigned
    bundle dir).
48. Plugin story: stable ABI or an officially source-level story; a real
    `lumen add` (currently appends `crate = "*"`); self-testing gallery.
49. Fuzz targets for .lss/agent/asset parsers; crash-report hook; opt-in
    telemetry.

## 6. Recommended documentation fixes (cheap, high-leverage)

1. Re-mark the task graph: T0.9 (macro), T2.3/T2.4 → PARTIAL; T3.1/T3.3/T3.4,
   T5.1/T5.2, T4.3, T6.1–T6.6, T7.1–T7.5 → re-scoped or un-checked to match
   backlog.md / cross-platform-readiness.md (which are accurate).
2. Rewrite 03 §3/§4 to describe the real transport (shell TCP + env var),
   mark unimplemented methods "planned", and document the 15 implemented-but-
   unspecced methods (getDeps/whatDependsOn/lastChange/lint/probe/…).
3. Fix `lumen-shell-android/src/lib.rs:2-3` (claims touch input that
   `imp.rs:65` drops).
4. Amend 02 §3 to the `LeafWidget` + composite-fn reality (or keep the spec
   and schedule the trait); same decision for `App::run` and `#[lumen::test]`.
5. Update 04 with an "implemented subset" table (or an honest status column)
   — today the spec reads as shipped while ~75 % of properties are parse-only.
6. 08-examples-plan §4: mark anchored dropdowns landed (pick_list).

## 7. Bottom line

The verification core the project's thesis depends on — semantic tree,
selector engine, deterministic CPU renderer, damage contract, headless
harness, agent introspection — is real, tested, and in several places richer
than its spec. The overstated docs cluster in three places: **milestone
checkboxes** (M5–M7 ☑ for headless models of OS/hardware features), the
**styling runtime** (spec-complete grammar, ~25 %-applied properties), and the
**dev-loop plumbing** (wire protocol, test macro, live hot-patch, platform
shells). The single most misleading doc is the task graph; the single largest
code gap behind a "done" claim is the `.lss` property runtime; and the two
items that block the most downstream claims are the F2 retained pipeline
(perf + incremental-layout claims) and the dev-server transport (tier-2/3,
mobile test proxying, CLI-hosted agent, web bridge all assume it).
