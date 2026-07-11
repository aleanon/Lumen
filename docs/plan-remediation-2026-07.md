# Plan: remediation of the July-2026 review findings

*2026-07-08. Consolidates the three reviews into one ordered implementation
plan: `review-goals-2026-07.md` (performance / resources / agent-verify
priorities), `review-docs-vs-code-2026-07.md` (49-item docs↔code gap list),
and `review-agent-skills-2026-07.md` (the 8-skill suite). Includes the
documentation truth-pass and cleanup.*

Conventions: task IDs are `<phase>.<n>`. Size: **S** (≤half day), **M**
(1–2 days), **L** (multi-day), **XL** (multi-session, own sub-plan).
`→` = hard dependency. Every task cites the review item(s) it resolves —
`[D#n]` = docs-vs-code §5 item n, `[G]` = goals-review recommendation,
`[S]` = skills assessment. Commit per task (AGENT.md); every task that
changes behavior updates the affected spec/skill in the same commit — that
rule (not a final doc pass alone) is what keeps the docs true this time.

## Phase ordering & rationale

```
D0 doc truth pass ─┬─ S0 P0 skills (encode today's reality)
                   │
A engine keystone ─┼─ C agent/dev-loop ─┬─ T test harness
(styles→layout,    │  (auto-wait, logs, │  (macro, retry,
 F2 retained,      │   session, proto)  │   diff.png, fonts)
 incremental)      │                    │
        ↓          │                    ↓
B styling runtime  │              W widgets & API
        ↓          │                    ↓
R rendering perf ──┴── P platforms ── M media/motion/text ── E ecosystem
                                 ↓
                     D9 final doc reconciliation + skill refresh
```

- **D0 first**: cheapest, stops agents failing against lying docs today.
- **S0 immediately after**: skills encode *current* reality; they are updated
  per-phase as reality improves (the per-task doc rule covers them).
- **A before B**: applying `.lss` layout properties requires computing styles
  before layout — the same pipeline surgery as the F2 retained graph; do the
  surgery once.
- **C parallel to A/B**: the agent endpoint is independent of the rebuild
  pipeline.
- **Decision gates** (`ADR-*` tasks) precede every dependency escalation and
  every spec-vs-code divergence where the *spec* might be what changes.

---

## Phase D0 — Documentation truth pass (S–M each; no feature code)

The drift audit's §6, expanded. Goal: after D0, an agent reading any doc
gets reality, with unimplemented things explicitly marked *planned*.

- **D0.1 (M)** Re-mark `.ai_docs/06-task-graph.md`. Introduce a `◐ partial`
  legend. T0.9 ◐ (no `#[lumen::test]`), T2.3/T2.4 ◐ (library, no live
  orchestration / no process driver), T3.1 ◐ (no touch/IME), T3.3/T3.4 ◐
  (headless-only iOS), T3.6 ◐, T5.1 ◐ (CPU-wasm golden leg only), T5.2 ◐
  (headless model, zero OS wiring), T4.3 ◐ (no platform adapter), T6.1 ✗
  (placeholder), T6.2 ◐ (PNG+SVG-subset), T6.3 ◐ (stubs), T6.4 ◐ (unwired),
  T6.5 ◐, T6.6 ◐ (R4 parked, no cold-start/mem gates), T7.1 ◐, T7.2 ◐,
  T7.3 ◐, T7.4 ✗ (checklist only), T7.5 ◐ (auto-repair real; linker/design-
  import absent). Each ◐/✗ links the matching backlog/plan entry.
- **D0.2 (M)** Rewrite `.ai_docs/03` §3–§4: document the implemented method
  table (incl. the 15 unspecced methods: getDeps/whatDependsOn/lastChange/
  lint/probe/probeRegion/getMenu/menu.invoke/systemRequests/getWindows/
  setLocale/invokeAction/drop/clipboard/element-zoom); move unimplemented
  methods to a "planned" section; describe the real transport (shell TCP +
  `LUMEN_AGENT_ADDR`, WebSocket test path); §4 marked *design, not
  implemented* pending ADR-D2 (task C.6).
- **D0.3 (S)** `.ai_docs/04`: add a per-property **status column**
  (parsed / applied / rendered) from the audit table; banner: "layout
  properties do not reach layout yet — see plan-remediation Phase A/B".
- **D0.4 (S)** `.ai_docs/02` amendments (pending the W.1/W.4 ADRs): note
  `LeafWidget` + composite-fn reality, `RunExt::run(size)`, missing
  `#[component]`/`#[state_registry]`/`Checkpoint`, dead diagnostics
  (W0001/W0301/E0103), snapshot-feature State bound.
- **D0.5 (S)** Point fixes: `lumen-shell-android/src/lib.rs:2-3` touch
  claim; `.ai_docs/08` §4 (pick_list anchored dropdown landed);
  `.ai_docs/01` §3 "incremental: dirty subtrees only" and §2 GPU "layer
  caching / partial redraw" reworded to current truth + plan pointers;
  01 §8 CLI list matches `new/run/test/package/add`.
- **D0.6 (S)** Doc inventory hygiene (assessed — nothing warrants deletion):
  add status headers instead. `app-framework-readiness.md` → "historical
  analysis (2026-06-18); live status in backlog.md". `00-HANDOFF-README.md`
  → add note that 02–05 normative status is qualified by per-section
  *planned* markers (post-D0.2–D0.4). `plan-fine-grained-view.md` /
  `plan-rendering-performance.md` → refresh status headers (F2 remaining;
  R4 parked, R5 partial, R1.3 tessellation cache unbuilt).
  `plan-executor-and-renderer-generics.md` already accurate. Keep
  `09-flutter-widget-reference.md` (reference), `a11y-checklist.md`,
  `cross-platform-readiness.md`, `api-audit-1.0.md` (accurate). Delete: the
  two stale `crates/lumen-test/target/lumen-traces/*.jsonl` files already
  deleted in the working tree — commit that deletion; add `target/` ignore
  check.
- **D0.7 (S)** Adopt the **doc-currency commit rule** in `AGENT.md`: any
  commit changing public behavior updates the affected spec section, skill
  table, and task-graph mark in the same commit.

**Acceptance:** an agent given only `.ai_docs/` + `docs/` finds no claim the
drift audit would flag; `rg "planned" .ai_docs` shows every gap marked.

## Phase S0 — P0 skills (write now against current reality) [S]

- **S0.1 (M)** `scripts/agent_client.py` — the reusable socket client
  (connect/rpc/screenshot/wait-for-port helpers) extracted from
  writing-widgets 6c; skill snippets call it.
- **S0.2 (L)** Skill `verifying-apps` (assessment §3.3): verification
  ladder, implemented-method cheat sheet, no-auto-wait/poll pattern,
  node-N-selector trap, app.perf-is-zeros workaround, tofu doctrine,
  port lifecycle, trace files, `--lib` trap, golden workflow.
- **S0.3 (M)** Skill `styling-lss` (§3.2): the applied-property table
  (generated from the audit; regenerate per Phase-B task), tokens/themes,
  run-hot workflow, working-vs-dead constructs, diagnostics reality.
- **S0.4 (L)** Skill `building-apps` (§3.1): project shape, import reality,
  composition + honest widget-availability table, state rules, app-level
  modules, stable-id discipline, no-HTTP-yet pattern.
- **S0.5 (M)** Skill `debugging-lumen` (§3.4): symptom→tool map,
  introspection order, live-vs-dead diagnostics, panic behavior.
- **S0.6 (M)** Update `writing-widgets`: point Steps 5–6 at
  `verifying-apps`; add pick_list reference; example→lumen-widgets
  promotion path.
- **S0.7 (M)** Skill-drift gate: `crates/skills-smoke` test crate compiling
  extracted skill snippets; wire into `just check`. [S §4.7]
- **S0.8 (M)** `lumen new` templates the app-facing skills
  (building-apps/styling-lss/verifying-apps/debugging-lumen with adjusted
  paths) into scaffolded projects' `.claude/skills/`. [S §1]

**Acceptance:** a fresh agent session builds + verifies a toy app using only
the skills (dogfood run, scripted like the accordion exercise).

## Phase A — Engine keystone: one pipeline surgery (XL, own detailed plan)

Resolves the top item of all three goals-review dimensions and unblocks
Phase B. Write `docs/plan-retained-pipeline.md` first (A.0) with its own
subtask graph; the outline:

- **A.0 (M)** Sub-plan + coherence-oracle extension (`assert_view_coherent`
  must gate every step; add style/layout coherence variants).
- **A.1 (S)** **Hover/focus/pressed keyed into reactivity** — stop
  `clear_view_caches()` on pointer motion (app.rs:523-525); flags become
  signal-backed so F1 memoization survives hover. [G perf#2, D#37]
- **A.2 (L)** **Styles before layout**: compute styles from role/id/class/
  state *pre-layout*; merge `.lss` layout properties into the node's
  `LayoutStyle` (parse grid track lists; per-side padding/margin;
  flex-*/align/justify/min-max/aspect/position/inset/overflow). This is
  drift item [D#11] and the precondition for honest `.lss`. Golden impact
  audited via R0 corpus.
- **A.3 (XL)** **F2 retained node graph**: retain `Tree`/`meta`/semantics/
  dep-index across pumps; re-run only dirty `cx.scope` subtrees and splice
  (plan-fine-grained F2, unblocked by the A.2 reordering). Memo hits return
  `Rc<Element>`/COW instead of deep clones; intern scope/signal keys.
  [G perf#1/#6, D#37]
- **A.4 (L)** **Incremental layout**: dirty-subtree `relayout_subtree` in
  the live pump (exists, test-only today) once the tree is retained.
  [D#1, 01 §3 claim]
- **A.5 (M)** **Only-affected-node restyle** on state flips + per-node style
  memo keyed by (role,id,classes,states,sheet-gen). [G perf#7, D#16]

**Acceptance:** one-of-N-rows change re-runs O(changed) (counted, benched);
hover storm over the gallery causes zero full rebuilds; `.lss`
`width/padding/flex` visibly affect layout; all goldens byte-identical or
re-approved via R0 diff; `scope_memo_one_of_many` improves again.

## Phase B — Styling runtime completion (after A.2; parallelizable)

- **B.1 (M)** Nested rules applied (`&:hover`, `&.class`); extend grammar
  for `& > .part`. [D#12]
- **B.2 (M)** Live `MediaContext` (window size/scale/platform/pointer)
  gates `@media`; `@media container(...)` + `.container()` marker. [D#13]
- **B.3 (L)** Visual properties applied end-to-end: background gradients,
  shadow lists (fix comma grouping) , opacity (exists→paint), 1–4-value
  radius, per-side borders, transform(+origin), filter, blend-mode, clip,
  z-index, visibility, cursor. [D#15]
  *(Amendment 2026-07-10: landed — opacity (B.3a), single shadow (B.3b;
  comma lists degrade to the first shadow, `inset` disables), visibility
  incl. semantics (B.3c), 1–4-value radius (B.3d), linear/radial gradients
  (B.3e), per-side padding/margin longhands (B.3f), clip (B.3g),
  blend-mode (B.3h), per-side borders as strips (B.3i). Parked pending
  renderer/shell machinery, revisit with R.1–R.3: `filter` (PushLayer
  field + GPU shader), `transform`/`z-index` (hit-test + paint-order
  design), `cursor` (winit shell). These stay documented parse-only.)*
- **B.4 (L)** Typography to the text stack: font-family/style/features/
  variation, font-size/weight (measure pass), line-height, letter-spacing,
  text-align/overflow/wrap/decoration, selection-color. [D#15]
- **B.5 (L)** *(Amendment 2026-07-10: B.5a shipped — `transition:` plays
  via an id-keyed PropAnim engine in the runtime (not the Scheduler):
  paint-tier props, smooth retargeting, delay, reduced-motion snap, wired
  through both rebuild and the A.5 restyle path. Identity is StableId — the
  A.3.3 dependency dissolved. B.5b shipped same-day: keyframes evaluator
  (`animation:` with count/infinite/alternate/delay, per-segment easing,
  fill-forwards), automatic 150 ms theme-switch color animation seeded
  from old computed values, `animation-force`. B.5 complete.)*
  Motion wiring: `transition:` → Scheduler; keyframe evaluator
  (`@keyframes` playback); theme-switch 150 ms color animation; OS
  reduced-motion → `Scheduler.reduced_motion`; `animation-force`. [D#14,17]
- **B.6 (M)** Cascade origins: framework-default sheet, theme origin,
  typed inline `Style` via `.style()` (`Origin::Inline`); full state
  vocabulary (`:hover` alias, disabled/pressed/checked). [D#16]
- **B.7 (M)** Polish batch: relative colors `oklch(from … calc(…))` + `+`
  token + `rgb()` alpha; widget parts (`.track`/`.thumb` + `cx.part()`);
  `Style` setter parity + real set-equality parity test; KNOWN_PROPERTIES
  fixes (border-width/color); **E0103 emission**; unknown-unit diagnostic;
  `span` in `get_styles`. [D#17,18, D#7]
- **B.8 (S)** Regenerate the `styling-lss` skill table + 04 status column
  (per-task rule, but verify at phase end).

**Acceptance:** the 04 §4 example stylesheet (tokens/themes/nesting/hover/
transition) works verbatim; property-table "rendered" column ≥90 %;
`style_parity!` asserts set equality.

## Phase C — Agent protocol & dev loop (parallel with A/B)

- **C.1 (M)** **Live auto-wait**: 05 §3 conditions inside `resolve_action`
  + `ui.waitFor {selector, state?, text?, timeout_ms}`; `timeout_ms` on all
  actions. [G agent#2, D#19]
- **C.2 (M)** `app.logs` (ring buffer + `log` facade capture incl. panics)
  and real `app.perf` from `FrameStats` (p50/p95, frames, node_count).
  [G agent#3, D#20]
- **C.3 (S)** Shell routes through `Session` → live `session.exportTest`;
  accept `node-N` runtime ids as selectors; `NotFound` errors carry
  nearest-miss suggestions in readable form. [G agent#4/5, D#22,23]
- **C.4 (L)** Method-gap batch: `state.get`, `events.subscribe` (tree/
  input/reload/log/diagnostic notifications), `input.drag` (node-to-node),
  `input.hover`, `input.gesture` (tap/long-press/pan/pinch — synthesis
  exists in tests), `app.setValue`, `app.command` + `cx.register_command`,
  `session.start/stop`, `reload.apply`; param gaps (click pos/button/count,
  type clear, scroll dx/to, getTree selector, screenshot max_width). [D#21]
- **C.5 (M)** Client & security: `lumen agent call <method> <json>` CLI;
  thin **MCP server** over the protocol (make `mcp_manifest` real); bearer
  token for non-loopback binds. [D#23, D#26]
- **C.6 (S)** **ADR-D2 (decided): rewrite 03 §4** to the in-process
  watcher + shell-endpoint design (fold into D0.2's rewrite) + decision-log
  entry; a minimal socket protocol is built inside C.7/P.2 when its first
  consumer (tier-2 push, device proxying, web bridge) lands. [D#24]
- **C.7 (L)** Tier-2 live orchestration (watch → incremental `cargo build`
  → `dylib_update` push → swap in running app; abi-hash tier-3 downgrade)
  and tier-3 process-level restart driver (`restart_request` +
  `state_snapshot` handoff). Depends on ADR-D2. [D#25]
- **C.8 (M)** Lifecycle & CLI: `LUMEN_AGENT_ADDR=…:0` + bound-address
  discovery file, `app.quit` RPC, `just stop-agent`; CLI `inspect`,
  `agent serve`, `test --platform gpu`. [D#26, G agent]

**Acceptance:** the 03 spec (as rewritten in D0.2) and the code agree 1:1;
a scripted agent run against an animated app passes with zero sleeps
(auto-wait only); `session.exportTest` from a live window compiles+passes.

## Phase T — Test harness (parallel; small)

- **T.1 (M)** `#[lumen::test]` attribute macro (+ `size/scale/theme/
  platform` opts; platform ⇒ `#[ignore]` without runner). [D#27]
- **T.2 (M)** Locator/expect completion: right_click, type_text,
  scroll_into_view, to_be_visible, `TestApp::run_command`; **make every
  `expect` assertion retry** (shared poll helper). [D#28]
- **T.3 (M)** Goldens: write `.diff.png`; GPU perceptual compare (ΔE Oklab
  ≤2.0, ≤0.1 % pixels) for the `exact_vs_cpu` corpus + opt-in per test.
  [D#29]
- **T.4 (M)** Fonts & tofu: subset a Latin+symbols default face (~1–2 MB)
  with the glyphs widgets actually use (chevrons!), pan-Unicode face
  behind a feature/`App::with_font`; `ui.lint` gains glyph-not-found
  (tofu) detection. [G resources#1, D#30, G agent#7]

**Acceptance:** hello release ≤7 MB (≤5 MB with `strip`, R.4); accordion
chevrons render; a flaky-timing test written naively passes via retries.

## Phase W — Widgets & core API (after ADRs; parallel with B)

- **W.0 (S)** **ADR-W1 (decided): amend 02 §3** to bless `LeafWidget` +
  composite fns as the widget model; write the decision-log entry; **add
  the missing leaf `event()` hook** to `LeafWidget`. [D#2]
- **W.1 (L)** Missing M2 widgets: Popover (generalize pick_list's anchored
  overlay: screen-edge flipping, dismiss), Sheet, Drawer, SearchField;
  promote Toast/Spinner/Chip from examples. [D#10]
- **W.2 (L)** Missing M4 widgets: Combobox (Popover + filtering),
  ColorPicker, Skeleton, Avatar, Pagination, RangeSlider, FilePicker
  (portable `SystemRequest` until P.4), pie chart; promote line chart;
  standalone Align. [D#10]
- **W.3 (M)** Builder/API parity: `.key()`, generic `.on(EventKind, h)`,
  `.style(Style)` typed inline (pairs with B.6); `cx.memo`/`cx.effect` on
  BuildCx; `#[component]` macro (PartialEq-props over a composite fn).
  [D#3,4,5]
- **W.4 (M)** State formalization: `Checkpoint` trait wrapping existing
  snapshot fns; `#[state_registry]` for stored trait objects; emit W0001
  (duplicate StableId) and W0301 (unnamed focusable) as diagnostics;
  `App::run` shape per ADR-W1 companion decision. [D#6,7,8]
- **W.5 (S)** **ADR-W2 (decided): bless direct crate imports in-repo**;
  `lumen new` scaffolds facade-only; amend 02 §11 + decision-log entry.
  (No 91-file migration.) [D#9]

**Acceptance:** 02 §10 widget table has no "missing" row for M0–M4; every
new widget follows writing-widgets (test triple + example + live smoke).

## Phase R — Rendering & resource follow-ups (after A)

- **R.1 (M)** GPU damage scissor (damage already computed; set scissor +
  preserve swapchain / persistent-texture + partial blit). [G perf#4]
- **R.2 (M)** Lyon tessellation cache (path+style hash — the R1.3
  promise); persistent uniform/instance buffers (ring allocator);
  ImageId/content-hash texture + ramp cache; skip root blit when no
  backdrop layers; cross-frame layer caching where layers are stable.
  [G perf#4, D#38]
- **R.3 (M)** CPU path: cull DrawCmds against damage rect pre-raster; wire
  `cull_visible` into paint. [G perf#3, D#38 adj.]
- **R.4 (S)** Release profile `strip = true` — **`panic = "abort"` is
  excluded** (discovered at implementation: E0701/`error_boundary`
  containment requires `catch_unwind`; abort would kill the process on any
  contained build panic). `size_gate.sh` fails on budget once the T.4
  subset font makes <5 MB reachable; add to CI then. *strip landed
  2026-07-09.* [G resources#1, D#40]
- **R.5 (M)** Cache hygiene: LRU/half-eviction for shape/run/glyph/shadow
  caches; byte-caps for image-valued caches; lazy/capped task pool
  (min(cores,4), spawn-on-first-job). [G perf#5, resources#4/5]
- **R.6 (M)** Gates: cold-start bench (<300 ms) + memory/leak gate in CI;
  put GPU-parity + golden suites in CI. [D#40, D#36]
- **R.7 (S)** **ADR-R1 (decided):** R4 threaded layout parked
  (virtualization is the answer); `Backend::VelloCompute` marked
  *evaluation slot, post-2.0*; both reflected in 06-task-graph (T6.1
  ✗→planned, T6.6 rescoped) with the **binding revisit triggers** recorded
  in the decision-gates table below. [D#39,41]

**Acceptance:** small-update GPU frame re-renders only the damage region
(measured via timestamp queries or frame capture); hello ≤5 MB stripped;
perf_gate + size_gate + cold-start green in CI.

## Phase P — Platforms (this dev box: X11 desktop + Android emulator + GPU;
iOS remains build-only)

- **P.1 (L)** Android input: touch → input queue (replace `imp.rs:65`
  drop), soft-keyboard/IME, safe-area insets, back button; verify on the
  local emulator (`android-env.sh`); re-enable a Linux-side `mobile.yml`
  job (emulator headless) or a `just android-gate` local recipe. [D#31,36]
- **P.2 (L)** Web: wasm event-loop shell (winit-web or hand-rolled RAF
  loop), input events, CPU present via canvas first, WebGPU present as
  follow-up; agent bridge over WebSocket; headless-Chromium test leg;
  enforced wasm size gate. [D#33]
- **P.3 (M)** **ADR-P1 (decided): arboard/rfd/muda approved** (decision-log
  entries at landing; rfd Linux backend sub-decision at P.3b — portal
  backend, async dep contained to lumen-shell, GTK fallback if it leaks).
  Slices in testability order, each behind the existing
  portable APIs and live-window-verifiable on this box: arboard clipboard
  (P.3a), rfd file/color dialogs (P.3b), muda native menus (P.3c),
  multi-window (P.3d: shell loop refactor keyed by WindowId, `WindowDesc`
  realized, per-window scale), OS drag-and-drop + tray + notifications
  (P.3e). Update backlog A4 (this box *can* verify live — the
  sandbox-blocked grading is stale for desktop). [D#34]
- **P.4 (M)** AccessKit bridge: `accesskit_winit` adapter publishing the
  existing `TreeUpdate`, AT actions → `inject()`; acceptance = adapter
  tree ≡ `semantics_json` diff test; live Orca smoke *attempted* on this
  box (GNOME session), documented if unavailable. [D#35]
- **P.5 (S)** iOS: keep headless leg; implement the referenced-but-missing
  FFI stubs so the template at least compiles honestly, or delete the
  template claims (D0.5 interim). Full shell stays blocked (no macOS).
  [D#32]

**Acceptance:** settings app on the Android emulator responds to touch +
soft keyboard; web example runs interactively in Chromium driven via the
agent bridge; clipboard/menu/dialog/multi-window driven live by the agent
on this box; AccessKit diff test green.

## Phase M — Media, motion, text, examples tail

- **M.1 (M)** **ADR-M1 (decided): `image` crate, default-features off** —
  jpeg + gif + webp, feature-gated for lean builds; avif deferred. Shared
  decode cache; animated-image asset type (frame sequences + clock);
  `ferris` + full `image` examples. [D#42, 08 tail]
- **M.2 (L)** SVG completion: gradients, transforms, groups, text,
  clips (or ADR to adopt usvg). Lottie: de-scope to post-2.0 (ADR-M1
  addendum). [D#42]
- **M.3 (M)** Motion wiring: route-level shared-element transitions
  (nav::Router integration), gesture bindings driving `SharedElement`
  fraction; keyframes covered by B.5. [D#44]
- **M.4 (L)** Rich text: lists/links/images in `RichDoc` (tables optional),
  caret/selection editing in `rich_text_editor` on top of RichDoc,
  find/replace UI; spell-check/variable-axes/CRDT explicitly *planned*.
  [D#45]
- **M.5 (M)** **ADR-M2 (decided 2026-07-08): no framework HTTP client.**
  The framework ships the *executor seam*, the user brings the transport
  (reqwest on their runtime, ureq on the thread pool, fetch on wasm —
  their call). Work items: (a) generic executor surface with the
  platform-conditional `MaybeSend` bound (native `Send`, wasm `!Send`) so
  one trait fits tokio handles, the thread pool, and `spawn_local`;
  (b) `WasmSpawner` (`spawn_local`); (c) fix or remove the noop-waker
  `Runtime::resource` poll path (state.rs:540) — resources are
  completion-based via `Sink`, the framework never drives foreign wakers;
  (d) harden the re-entry contract: `Sink` is the only `Send` handle,
  stale-generation discard documented + tested; (e) the canonical
  bring-your-own-client recipes live in the `lumen-data-async` skill;
  `pokedex`/`download_progress` examples use a client as **dev-deps**
  (nothing ships in the framework tree). [D#46, backlog Part D]
- **M.6 (S)** Examples tail: QR encoder (pure-Rust dep via ADR-M1),
  vectorial text (swash outlines → Canvas), sysinfo (feature-gated),
  exit/url_handler/multi-window/integration examples (needs P.3d).
  [D#46]
- **M.7 (—)** **ADR-M3 (decided):** audio/video/capture de-scoped to
  post-2.0; stubs stay as the deterministic CI contract; T6.3 re-marked
  in D0.1. [D#43]

## Phase E — Ecosystem & production

- **E.1 (M)** Packaging: AppImage (verifiable on this box) from
  `lumen package`; msix/dmg/ipa + signing/notarization stay blocked
  (CI-secrets infra) — re-marked per D0.1; SBOM via `cargo auditable` +
  existing cargo-deny. [D#47]
- **E.2 (M)** Plugin story per ADR-W1: bless source-level `LeafWidget`
  as the 1.x mechanism (stable *API*, not ABI); `lumen add` resolves a
  real version (crates-io/GitHub lookup) and registers the widget;
  gallery self-test includes an out-of-repo widget crate. [D#48]
- **E.3 (M)** Hardening: `cargo-fuzz` targets for `.lss` parser, selector
  parser, agent JSON dispatch, PNG/SVG decode; nightly fuzz job;
  crash-report hook (panic → structured diagnostic sink); telemetry
  explicitly *not planned* (privacy stance, doc'd). [D#49]

## Phase D9 — Final reconciliation

- **D9.1 (M)** Re-run the drift audit (same 3-agent method) against the
  finished tree; fix any residue.
- **D9.2 (S)** Remove *planned* markers that landed; flip 06 marks with
  evidence links; refresh all skill tables; re-run the S0.7 snippet gate.
- **D9.3 (S)** Update the three review docs with a "resolved by" column
  pointing at commits — they become historical records.
- **D9.4 (S)** Write `docs/review-goals-<date>.md` v2 scorecard (idle CPU,
  binary size, changed-frame O(), agent-run flake rate) proving the
  goals-review liabilities closed.

---

## Decision gates — ALL DECIDED 2026-07-08 (user approved the recommendations)

No phase is blocked on a decision. Each gets a formal `07-decision-log.md`
entry when its phase starts (per ADR-003, before any dep is added).

| ADR | Decision (final) | Affects |
|---|---|---|
| ADR-W1 | **Amend 02 §3 to the shipped model** (`LeafWidget` + composite fns); **add the missing leaf `event()` hook** to `LeafWidget` | W.0→W.*, E.2 |
| ADR-W2 | **Bless direct crate imports for in-repo examples** (they are framework tests); `lumen new` scaffolds are facade-only; amend 02 §11 | W.5 |
| ADR-D2 | **Rewrite 03 §4** to the watcher + shell-endpoint design now; build a minimal socket protocol only when a consumer exists (tier-2 push / device proxying, C.7/P) | C.6→C.7, P.2 |
| ADR-P1 | **Approve arboard + rfd + muda**, landed in testability order (clipboard → dialogs → menus → multi-window). Sub-decision at P.3b: rfd's Linux backend — default to the xdg-portal backend with its async dep (zbus) contained to `lumen-shell`; fall back to GTK if the executor leaks beyond the shell crate | P.3 |
| ADR-M1 | **`image` crate, default-features off**: jpeg + gif + webp; **avif deferred** (pure-Rust decoder immaturity); Lottie de-scoped to post-2.0; QR via `qrcodegen`-class pure-Rust dep. Codecs feature-gated so lean builds drop them; versions workspace-pinned (golden stability) | M.1, M.2, M.6 |
| ADR-M2 | **No framework HTTP client** — executor seam only (`MaybeSend` trait, `WasmSpawner`, Sink re-entry contract); user brings the client; recipes live in the `lumen-data-async` skill; examples use clients as dev-deps | M.5 |
| ADR-M3 | **Audio/video/capture de-scoped to post-2.0**; deterministic stubs remain the CI contract; T6.3 re-marked in D0.1 | M.7 |
| ADR-R1 | **R4 threaded layout and Vello stay parked**, with explicit revisit triggers (below); T6.1/T6.6 re-marked in D0.1 | R.7 |

**ADR-R1 revisit triggers (binding):** R4 — a real app's dirty-subtree
relayout exceeds the 2 ms budget *after* Phase A lands, or the cold-start
gate shows a >50 ms initial layout on a production screen. Vello — a
path-heavy *animated* bench (morphing paths, not cacheable by R.2's
tessellation cache) misses frame budget on target mobile hardware, or
Vello reaches a stable release whose WebGPU compute baseline the supported
platforms meet.

## Coverage matrix

- Goals-review recommendations → A.1/A.3 (perf 1), A.1 (perf 2), R.3
  (perf 3), R.1/R.2 (perf 4), R.5 (perf 5), A.3 (perf 6), A.5 (perf 7);
  T.4+R.4 (resources 1), A.* (resources 2), R.2 (resources 3), R.5
  (resources 4/5); D0.2 (agent 1), C.1 (agent 2), C.2 (agent 3), C.3
  (agent 4/5), C.8 (agent 6), T.4 (agent 7), T.3 (agent 8).
- Docs-vs-code §5 items 1–49 → D#1:A.4 · D#2:W.0 · D#3:W.3 · D#4:W.3 ·
  D#5:W.3 · D#6:W.4 · D#7:W.4+B.7 · D#8:W.4 · D#9:W.5 · D#10:W.1/W.2 ·
  D#11:A.2 · D#12:B.1 · D#13:B.2 · D#14:B.5 · D#15:B.3/B.4 · D#16:B.6+A.5 ·
  D#17:B.5/B.7 · D#18:B.7 · D#19:C.1 · D#20:C.2 · D#21:C.4 · D#22:C.3 ·
  D#23:C.3/C.5 · D#24:C.6 · D#25:C.7 · D#26:C.8 · D#27:T.1 · D#28:T.2 ·
  D#29:T.3 · D#30:T.4 · D#31:P.1 · D#32:P.5 · D#33:P.2 · D#34:P.3 ·
  D#35:P.4 · D#36:P.1/R.6 · D#37:A.1/A.3 · D#38:R.1–R.3 · D#39:R.7 ·
  D#40:R.4/R.6 · D#41:R.7 · D#42:M.1/M.2 · D#43:M.7 · D#44:M.3 · D#45:M.4 ·
  D#46:M.5/M.6 · D#47:E.1 · D#48:E.2 · D#49:E.3.
- Skills assessment → S0.1–S0.8 now; refreshed per phase; D9.2 final.

## Sizing summary & suggested waves

| Wave | Phases | Rough effort |
|---|---|---|
| 1 (immediately) | D0 + S0 | ~1 week of sessions |
| 2 (the keystone) | A (+ C.1–C.3, T.1–T.4, R.4 in parallel) | ~2–3 weeks |
| 3 | B + W + remaining C + R | ~3–4 weeks |
| 4 | P + M | ~3–4 weeks |
| 5 | E + D9 | ~1 week |

Waves 2–3 deliver the framework's own three goals (the goals-review
priority list closes entirely inside them). Waves 4–5 deliver the milestone
claims honestly. Every wave ends re-running: `just check`, perf_gate,
size gate, the skills-smoke gate, and one scripted agent gauntlet.
