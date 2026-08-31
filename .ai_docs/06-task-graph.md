# 06 — Task Graph & Acceptance Criteria

Topologically ordered. `Deps` are hard prerequisites. Acceptance = listed commands exit 0 in CI (Linux + Windows + macOS unless noted). M0 is fully decomposed; M1–M4 are decomposed to PR-sized tasks but with coarser acceptance — refine each into subtasks (recorded in this file) when you start the milestone.

Legend: ☐ open ☐→ in progress ☑ done ◐ **partial** (a real, tested slice
shipped; the rest is planned) ✗ **not implemented** (placeholder/model only).
Update checkboxes in the task's merge commit.

> **Status re-mark (2026-07-09).** The 2026-07 docs↔code audit
> (`docs/review-docs-vs-code-2026-07.md`) found many ☑ marks covered only the
> headless/deterministic slice of their task while the OS/hardware/CI half was
> unbuilt. Those marks are corrected to ◐/✗ below, each with a one-line
> reality note pointing at `docs/backlog.md` and the remediation plan
> (`docs/plan-remediation-2026-07.md`, task IDs like P.1/C.7). The acceptance
> texts are left as written — they remain the bar for flipping back to ☑.

---

## M0 — Foundations & verification tools
Build the eyes first: by the end of M0 every later task can be verified headlessly.

**T0.1 ☑ Workspace scaffold + CI.** Deps: —
Workspace with all 11 crates (02 §1) compiling empty; `rust-toolchain.toml`; CI (GitHub Actions): fmt, clippy `-D warnings`, test on linux/windows/macos; `deny.toml` license check; `lumen-core/diagnostics.md` seeded with codes from 02 §9.
*Accept:* `cargo build --workspace && cargo clippy --workspace -- -D warnings` green on 3 OS runners.

**T0.2 ☑ Node tree + SoA hot data.** Deps: T0.1
Generational `NodeIndex`; intrusive tree links + parallel arrays per 02 §5; insert/remove/reparent; document-order and z-order iteration; hit-test scan honoring clip/flags.
*Accept:* `cargo test -p lumen-core tree::` — incl. property tests (proptest): 10k random tree edits preserve invariants (no dangling indices, parent/child symmetry); hit-test agrees with a naive reference implementation on 1k random scenes.

**T0.3 ☑ Signals + state store + checkpoint.** Deps: T0.2
`signal/memo/effect/resource` per 02 §4; identity-path keying (hash-folded `Hash + Debug` keys since ADR-021, 2026-08-02); batched writes; subscriber-only invalidation; `Checkpoint` impl: snapshot → restore round-trip; `#[state_registry]` macro for stored trait objects; W0002 lenient deserialization. *(Truth note: T0.3 shipped the round-trip as ad-hoc fns; the `Checkpoint` trait itself landed 2026-07-10 — plan W.4b, incl. live in-place restore. `#[state_registry]` shipped 2026-07-10 — plan W.4c.)*
*Accept:* `cargo test -p lumen-core state::` — incl.: writing 1 of 10k signals re-runs exactly 1 scope (counted); snapshot/restore of a 1k-signal store is lossless; struct-evolution fixture (field added/removed) restores with defaults + W0002.

**T0.4 ☑ Display list + CPU renderer.** Deps: T0.1
`DrawCmd` per 02 §7; tiny-skia execution: rects/rrects/borders, paths (fill/stroke), gradients (3 kinds), images, layers (clip/opacity/transform/blend), damage-region rendering. Bit-deterministic.
*Accept:* `cargo test -p lumen-render` — golden PNGs for each command class (exact compare); same scene rendered twice is byte-identical; damage test: re-render of dirty rect equals full re-render cropped.

**T0.5 ☑ Layout engine wrapper.** Deps: T0.2
`lumen-layout` over Taffy: style→Taffy mapping for the layout property set (04 §3), incremental relayout of dirty subtrees, results written into SoA `bounds`.
*Accept:* `cargo test -p lumen-layout` — fixture suite of 40 layouts (flex, grid, absolute, min/max, aspect-ratio) asserting exact bounds; dirty-subtree relayout touches only descendant nodes (counted).

**T0.6 ☑ Text v0.** Deps: T0.4
parley+swash wrapper: single & multi-style runs, wrapping, alignment, ellipsis, bundled Noto fonts (no system fonts in tests), glyph atlas for the CPU path; bidi + CJK fixtures from day one.
*Accept:* `cargo test -p lumen-text` — goldens for latin/CJK/bidi/emoji/wrap/ellipsis; measurement function returns stable sizes across runs.

**T0.7 ☑ Event routing + focus.** Deps: T0.2
Event enum per 02 §6; capture/bubble dispatch via SoA hit-test; pointer enter/leave tracking; Tab focus traversal; timer events; single input queue used by both OS and synthesized input.
*Accept:* `cargo test -p lumen-core events::` — dispatch-order fixtures; enter/leave on synthetic moves; focus ring traversal over 20-node fixture matches expected order.

**T0.8 ☑ Semantics tree + JSON export.** Deps: T0.2, T0.7
`SemanticsNode` building during rebuild; elision rules; schema per 03 §1 (validated against a JSON Schema file checked into repo); selector engine per 03 §2.
*Accept:* `cargo test -p lumen-core semantics::` — schema validation on fixtures; selector test table (≥30 cases incl. `:has`, `:nth`, ambiguity errors with candidates).

**T0.9 ☑ Headless app + harness seed. ← verification gate.** Deps: T0.3–T0.8
*(Re-completed 2026-07-09: the missing test macro shipped as `#[lumen_test::test]` with size/scale/theme/app/platform options — plan T.1.)*
`App::run_headless`, `Headless::{pump, inject, screenshot, semantics_json}` (02 §8); minimal `lumen-test`: `#[lumen::test]`, `TestApp`, `Locator` with click/fill/press/text, `expect` with to_exist/to_have_text, auto-wait per 05 §3, exact-golden `expect_screenshot`, virtual clock.
*Accept:* `cargo test -p lumen-test` self-tests: auto-wait succeeds on delayed-appearance fixture, fails `Ambiguous` with candidates on duplicate fixture; golden round-trip works; `LUMEN_UPDATE_GOLDENS` re-records.

**T0.10 ☑ Ten primitive widgets.** Deps: T0.9
Text, Image, Row, Column, Stack, Scroll, Button, TextFieldBasic, Checkbox, Slider — each: build/layout/paint/event/semantics, keyboard map, default styles (hardcoded constants until T1.2), rustdoc + example.
*Accept:* per-widget golden + semantic-tree + interaction test (e.g. slider: drag changes value; checkbox: space toggles; scroll: wheel moves content & updates `scroll` in semantics). `cargo test -p lumen-widgets`.

**T0.11 ☑ winit shell + wgpu renderer.** Deps: T0.4, T0.10
Desktop window, surface, resize/scale handling, vsync present, damage-aware redraw; glyph/image atlases on GPU; parity harness comparing GPU output to CPU goldens (perceptual threshold 05 §4).
*Accept:* `cargo test -p lumen-render -- --ignored gpu_parity` on GPU runner; `examples/hello` opens, renders the counter, idle CPU <0.5% over 10 s (measured in an ignored test on desktop runner).

**T0.12 ☑ CLI skeleton.** Deps: T0.9, T0.11
`lumen new` (scaffolds app with `main_app()` convention), `lumen run`, `lumen test` (wraps cargo test), all with `--json` output envelopes.
*Accept:* integration test: `lumen new demo && cd demo && lumen test --json` passes and emits valid JSON.

**M0-exit ☑:** `examples/hello` counter app; CI runs a lumen-test that queries the tree, clicks `#increment` by selector, asserts label `1`, matches exact golden — on all 3 desktop OS runners, headless.

---

## M1 — Usable desktop framework
**T1.1 ☑ `.lss` parser + cascade.** Deps: T0.10. Grammar 04 §1–2; atomic reject-on-error; E0101–E0104 with spans. *Accept:* parser test corpus (valid + 30 error fixtures asserting codes/spans/did-you-mean); cascade/specificity table tests.
**T1.2 ☑ Property set + Rust mirror.** Deps: T1.1. All v1 properties applied; `Style` typed API; `style_parity!` macro test; computed-value serialization 04 §7; widgets restyled via default `.lss`. *Accept:* `cargo test -p lumen-style`; goldens of widget gallery under light/dark.
**T1.3 ☑ Tokens, themes, media queries.** Deps: T1.2. *Accept:* theme-switch test animates colors; media-query fixtures at 3 window sizes.
**T1.4 ☑ Animation scheduler.** Deps: T1.2. Transitions, keyframes, springs; vsync-driven; virtual-clock control in tests; reduced-motion. *Accept:* frame-by-frame value assertions using TestClock; idle-after-settle test (0 frames once animations finish).
**T1.5 ☑ Full text input + IME.** Deps: T0.6. Editing model (selection, undo), preedit handling, clipboard; TextField/TextArea on it. *Accept:* IME preedit fixture tests (synthetic ImePreedit/TextInput sequences incl. CJK composition); goldens for selection rendering.
**T1.6 ☑ Widget library → 30.** Deps: T1.2, T1.5. List in 02 §10 M1; VirtualList with windowing. *Accept:* per-widget test triple (golden, semantics, interaction); VirtualList: 1M items, ≤ visible+overscan nodes materialized (counted), scroll goldens.
**T1.7 ☑ Dev server + tier-1 hot reload.** Deps: T0.12, T1.1. File watcher; wire protocol 03 §4; style/asset push; structured reload events. *Accept:* integration test: run app, modify `.lss` on disk, assert style changed via `ui.getStyles` within 500 ms and `reload` event received; broken edit keeps old style + E0101 event.
**T1.8 ☑ `lumen-agent` v1.** Deps: T0.9, T1.7. JSON-RPC/WebSocket server in dev server, proxied to app; observation set + click/type/key/scroll; annotated screenshots; MCP tool manifest. *Accept:* protocol conformance suite driving the counter app end-to-end over a real socket (golden JSON transcripts, tolerant of `seq`/timing fields).
**M1-exit ☑:** the "settings app" example (3 screens, themed, animated, IME text input) fully styleable from `.lss`, hot-reloads styles live, and is drivable by an external script through `lumen-agent`.

---

## M2 — Testing & AI loop complete
**T2.1 ☑ lumen-test full surface** (all of 05 §2: drag, set_value, styles/bounds assertions, perceptual GPU goldens, per-test size/scale/theme). *Accept:* harness self-test suite.
**T2.2 ☑ Traces** (05 §5) + failure artifacts. *Accept:* trace schema validation; failing test embeds screenshot+tree.
**T2.3 ◐ Tier-2 hot patch.** cdylib registry, incremental rebuild orchestration, libloading swap, state-preservation, abi_hash downgrade to tier 3, intentional dylib leak.
*(C.7 ◐ — regraded 2026-08-08, HR1: swap **mechanics** ✅ (`hotpatch.rs`, fixtures `hot_a/b/c`) AND live orchestration ✅ — `Tier2Driver` (`lumen-cli/src/dev.rs`): watch → incremental `cargo build -p` → `HotComponent::swap` into the RUNNING app, tier-3 in-process restart on downgrade; the `lumen dev` engine. But the **"abi gate" ✗**: `HOST_ABI_HASH` is a fixed placeholder fingerprinting neither compiler nor crate layout — Rust has no stable ABI to derive one from — so a matching token proves nothing. Tier 2 is therefore opt-in (`LUMEN_TIER2=1` / `set_tier2`) and **tier 3 is the default path**. See 01 §7.)* *Accept:* integration: edit a `build()` fn → swap <2 s on warm cache, counter state preserved; change state shape → that component resets, others preserved; core-crate edit → automatic tier-3 with state restore.
**T2.4 ◐ Tier-3 snapshot restart.** *Accept:* kill/rebuild/restore round-trip preserves signals, scroll, focus.
*(◐: state snapshot/restore real (`AppSnapshot`, `run_headless_restored`, tier3.rs test); the "kill/rebuild" is an in-process drop — no process-level restart driver; plan C.7.)*
**T2.5 ☑ `session.exportTest`.** Recording, codegen to lumen-test source, auto-assertions. *Accept:* recorded session on settings app exports a test that compiles and passes.
**T2.6 ☑ Perf gates.** criterion benches: 10k-node dirty-subtree layout <2 ms; 1M-row VirtualList scroll ≥120 fps equivalent frame budget on reference desktop runner; idle = 0 frames. CI regression gate ±10%. *Accept:* bench workflow green + gate script.
**M2-exit ☑:** an agent connected only to `lumen-agent` can explore the settings app, export a regression suite, and the suite runs green in CI on 3 OSes.

---

## M3 — Mobile
**T3.1 ◐ Android shell** (cargo-ndk, GameActivity, surface lifecycle, touch, soft-keyboard/IME, safe areas). *Accept:* hello app runs on API-34 emulator in CI (headless emulator), agent screenshot matches golden perceptually.
*(P.1 ✅ 2026-07-20: input is wired through the one queue — touch (down/move/up incl. multi-pointer actions), back = Escape (overlay dismissal; app survives), named keys + unicode text via the device `KeyCharacterMap`, soft keyboard shown/hidden on text-input focus, safe-area layout via the content rect (shrinks under the IME — and state now SURVIVES resize; the old shell rebuilt the app and dropped every signal), DPI scale from density/160, cleared+offset blit. Emulator-verified end-to-end and gated: `just android-gate` (build+install+launch, tap ⇒ pixels change, type ⇒ pixels change, back ⇒ alive). Still native-activity: true IME commit text (CJK composition) needs GameActivity — future work; mobile CI stays local-gate.)*
**T3.2 ☑ Android orchestration** (`lumen run --platform android`: AVD provision, build, install, log stream, adb reverse for dev socket). *Accept:* scripted end-to-end on CI emulator incl. tier-1 hot reload.
**T3.3 ◐ iOS shell** (UIKit host, Metal surface, touch/IME/safe areas, Xcode project template). *Accept:* hello app on iOS Simulator (macOS runner) with agent screenshot golden.
*(◐: headless `render_into()` only; template uses CoreGraphics (not Metal) and references FFI symbols that don't exist; no macOS/simulator on this box; see `docs/cross-platform-readiness.md`; plan P.5.)*
**T3.4 ◐ iOS orchestration** (`simctl` boot/install/launch/screenshot; dev socket). *Accept:* scripted e2e on simulator incl. tier-1 reload; tier-2 verified on simulator, documented as tier-3-only on physical devices.
*(◐: `scripts/ios_orchestrate.sh` exists but has never run against a simulator (no macOS); plan P.5.)*
**T3.5 ☑ Gestures + mobile widgets** (GestureEvent full params; BottomNav, NavigationRail, AppBar, pull-to-refresh, DatePicker, TimePicker; touch target ≥44 px audit). *Accept:* gesture synthesis tests (pinch/pan/long-press) + widget test triples on both emulators.
**T3.6 ◐ `lumen test --platform android|ios_sim`.** *Accept:* M0-exit test passes unmodified on both.
*(◐: a bash-script dispatcher that cross-compiles the test binary and pushes goldens via adb — not the specced TestApp-over-dev-socket proxying; iOS leg unexercised.)*
**M3-exit ◐:** settings app runs on Android emulator + iOS Simulator; same test suite green on desktop+both; agent loop (edit `.lss` → reload → screenshot) works against the Android emulator.
*(◐: the Android emulator leg is real (local) **with touch + soft keyboard (P.1)**; the iOS-Simulator leg has never run (P.5 shipped headless FFI only).)*

---

## M4 — Depth & 1.0
**T4.1 ☑ ShaderWidget** (WGSL, typed uniforms, built-ins, CPU fallback fill, shader hot reload, E0201 diagnostics). *Accept:* GPU-runner goldens for 3 sample shaders; broken-shader edit keeps old pipeline + diagnostic.
**T4.2 ☑ DataGrid + Tree + charts + RichTextEditor.** *Accept:* test triples; DataGrid 1M-row gate added to perf suite.
**T4.3 ☑ AccessKit integration** (role/state map per 03 §1; platform adapter landed in plan P.4: `accesskit_winit` in the shell, per-frame `update_if_active`, AT actions → input queue). *Accept met:* map table complete; adapter tree ≡ semantics diff test (node-for-node walk incl. bounds/children order); AT-SPI live smoke on this box (identity + names + `doAction` driving state). VoiceOver/NVDA manual runs still need mac/Windows hardware (`docs/a11y-checklist.md`). *(GX2, 2026-08-08: native dialogs/menus/tray are behind lumen-shell's default-ON `desktop-integration`; off drops the Linux GTK cluster — measured 70 → 5 shared libraries. The facade must hold lumen-shell as a **direct path dep**: Cargo ignores `default-features = false` on workspace-inherited deps, so the feature would otherwise stay on under `--no-default-features`.)* *(GX4, 2026-08-08: the adapter is **on by default but now opt-out-able** — `LUMEN_A11Y=0` or the GTK/Qt-standard `NO_AT_BRIDGE=1`. `accesskit_unix::Adapter::new` spawns a D-Bus thread unconditionally, so "dormant until an AT subscribes" was true of the published tree, not of the adapter; 03 §59 corrected. Measured on a live window: 13 threads / 12 socket fds default vs 11 / 10 opted out. It is a switch rather than a deferral because detecting an AT needs the same connection the adapter owns.)*
**T4.4 ☑ Inspector app** (tree view, style editor, animation scrubber, trace replay — built in Lumen). *Accept:* inspector drives itself via lumen-agent in a self-test.
**T4.5 ☑ Remaining widget set, API audit, rustdoc pass, 1.0 freeze.** *Accept:* `cargo doc` no warnings; public-API diff reviewed; semver-checks clean.
**M4-exit ☑ = 13 of `01-architecture.md`:** an agent, given only the CLI and lumen-agent, scaffolds an app, implements a multi-screen styled UI with one custom shader, verifies on desktop + both mobile emulators, generates a passing test suite from its own session, and fixes an injected layout bug using structured diagnostics — zero human intervention. Script this as `examples/agent-gauntlet/` and run it as the release gate.

---

## M5 — Ubiquity & App-Building (post-1.0: run everywhere, build real products)

*Theme: 1.0 ships a native desktop+mobile widget toolkit. M5 closes the three
gaps that stop teams shipping **real apps**: the framework doesn't run on the
**web**, doesn't integrate with the **OS** (windows/menus/clipboard/DnD), and
lacks the **app-level scaffolding** (i18n, routing, forms) every product needs.
New ADRs: web/WASM backend; RTL layout; routing & global-state model.*

**T5.1 ◐ Web / WASM target.** wgpu→WebGPU with a WebGL2 fallback; a canvas-only shell (no DOM widgets); the CPU reference renderer compiled to wasm for golden parity; agent bridge over WebSocket/`postMessage`; asset/font streaming; wasm size budget. *Accept:* the settings + inspector apps run in headless Chromium, driven unmodified through `lumen-agent`, matching a perceptual golden; `lumen run --platform web`; wasm bundle under a gated size.
*(P.2 ✅ 2026-07-20 for the interactive core: persistent wasm session (`lumen-shell-web` session_* API — one Headless per instance), input through the one queue (pointer/keys/text/wheel from real DOM listeners in `web/app.mjs`), event-driven RAF loop (renders only changed frames; idles otherwise), CPU→2D-canvas present, and the agent bridge — `lumen_agent::dispatch` compiled to wasm (`ws` feature off; auto-waits degrade to single-attempt, no `Instant` on wasm) exposed as `window.lumenAgent` + a dev WebSocket transport (`?agent=ws://…` + `scripts/web_agent_relay.py`, so the standard TCP agent tooling drives a live browser). Gated: `just web-gate` = wasm ≤24 MB + node session leg (agent-resolved pointer click 0→1→2) + headless-Chromium leg (real Brave, real CDP mouse events, asserted via the bridge). Still open for T5.1 ☑: WebGPU/WebGL2 present, settings+inspector perceptual goldens, `lumen run --platform web` wiring, font streaming.)*
**T5.2 ◐ Desktop system integration.** Multi-window + multi-monitor (DPI/scale per window), native menu bar + context menus, system tray, native file/color dialogs, rich clipboard (text/image/files), drag-and-drop intra- and inter-app, OS notifications — all behind portable APIs surfaced on the agent + synthesizable in `lumen-test`. *Accept:* a multi-window app driven by the agent (focus, menu invoke, DnD between windows); clipboard + drop events synthesized headlessly in a test triple.
*(◐: the portable model layer is real (`system.rs`, agent methods) **and the OS wiring is landing per plan P.3**: arboard clipboard bridge (P.3a), rfd file-open dialogs (P.3b), muda menus + portable accelerators (P.3c — menubar attaches on Windows/macOS; on Linux/winit no attachment point exists, accelerators + `menu.invoke` activate). P.3e ✅: OS drag-and-drop (winit XDND → the one `Drop` event), desktop notifications (`notify-send`, terminal fallback), system tray (tray-icon on a gtk thread; the tray context menu hosts the app `MenuModel` — ayatana registers no item without a menu; tooltip/title from `TrayTooltip`; clicks → `activate_menu` via loop-waking proxy events). P.3d-1 ✅: `App::window(desc, root)` + `Headless::open_window` — one Headless per window over the shared Runtime (own tree/layout/paint; cross-window reactivity = shared signals; tested with a cross-window click). P.3d-2 ✅: shell loop keyed by WindowId — every declared window opens as a real OS window with its own renderer/surface/scale and per-window input routing; input anywhere fans a redraw to all windows. Live-verified: two OS windows, a main-window menu action re-rendered the stats window (pixel diff; the reverse direction is the headless cross-window click test). Remaining for T5.2 ☑: per-window agent verbs (window param on ui.getTree/input.*) + agent-driven cross-window DnD; backlog A4.)*
**T5.3 ☑ Internationalization & RTL.** Fluent-style message catalogs with structured missing-key diagnostics; ICU-class locale formatting (date/number/plural/currency); **RTL layout mirroring** in `lumen-layout` (start/end resolution, logical insets); per-locale theming; agent `input.setLocale`. *Accept:* one app rendered in en / ar / ja with RTL-mirror goldens; locale switch via agent reflows + re-mirrors; missing-translation surfaces as a `W####` code.
**T5.4 ☑ Navigation, global state, undo/redo, persistence.** Typed router with a back stack + deep links + guards; global stores layered on the signal runtime; a command/undo-redo history; whole-app state save/load (building on the Checkpoint protocol). *Accept:* deep-link navigation + multi-step undo/redo driven by the agent; app state round-trips through save→relaunch and through a tier-3 restart.
**T5.5 ☑ Forms & validation.** Declarative form state, sync + async validators, input masks/formatters, error→diagnostic surfacing with accessible error association (a11y `described_by`). *Accept:* a validated multi-field form; the agent fills it, reads validation failures as **structured data** (not pixels), corrects them, and submits.
**M5-exit ◐:** an agent, given only the CLI + lumen-agent, scaffolds and builds a **localized (RTL+LTR), multi-window, routed, form-driven CRUD app**, runs it on **desktop + web + the Android emulator**, exercises undo and deep-links, and exports a passing cross-platform suite from its own session — scripted as `examples/agent-gauntlet-web/`, added to the release gate.
*(◐: the gauntlet runs **headless on desktop**; "multi-window" is the model layer, the web leg is the CPU-wasm golden only.)*

---

## M6 — Media, Motion & Performance (rich, fluid, fast at scale)

*Theme: M5 makes Lumen deployable; M6 makes it **feel premium** and pays down
the GPU/perf debt deferred from v1 — rich media (vector/video/audio), a
world-class motion system, and the compute-rasterization + multi-threading work
flagged as a v1 evaluation. New ADRs: Vello-class GPU backend; media pipeline;
motion/choreography model.*

**T6.1 ✗ Vello-class GPU rasterizer.** A compute-shader path/scene rasterizer behind the existing display-list contract (selectable vs the lyon path); multi-threaded scene building; CPU↔GPU perceptual parity preserved. *Accept:* a complex vector scene matches the CPU golden within threshold on a GPU runner; path-heavy perf gate beats the lyon baseline; idle/damage contracts unchanged.
*(✗: `Backend::VelloCompute` is a placeholder enum variant; the real GPU path is wgpu + lyon CPU tessellation. Only the backend seam + threaded viewport cull landed. **Parked post-2.0 per ADR-R1** with binding revisit triggers in `docs/plan-remediation-2026-07.md`.)*
**T6.2 ◐ Vector & image media.** SVG rendering, Lottie/animated-vector playback, GIF/APNG, and jpeg/webp/avif decode with a shared image cache/atlas; declarative asset references resolved by the dev server (tier-1 hot-swap). *Accept:* SVG + Lottie goldens at fixed clock; codec round-trips; a swapped asset reloads live.
*(◐→: PNG decode + cached assets real; **M.1 ✅ (ADR-M1): jpeg/gif/webp decode via the `image` crate** (pure-Rust decoders, default-on `codecs` feature, lean drops it) through the shared content-keyed cache; animated GIFs decode to `asset::Animation` and play on the virtual clock (`asset::animated`). **M.2 ✅: SVG completed dependency-free** — nested `<g>` inheritance, composed `transform` (translate/scale/rotate/matrix, flattened into geometry), linear/radial gradients from `<defs>`, rect `clip-path` (layer-clipped), fill+stroke with opacity, full path grammar `MmLlHhVvCcSsQqTtAaZz` (arcs → cubics via kurbo), `<text>` via a caller-supplied shaper so the text stack stays above lumen-render (usvg rejected: it would ship a second font stack). Documented-unsupported: filters/masks/patterns/non-rect clips/`use`. avif deferred, APNG unplanned, **Lottie de-scoped post-2.0 (ADR-M1 addendum)**.)*
**T6.3 ✗ Audio / video / capture.** A media pipeline: hardware-accelerated video decode where available + a deterministic software path for CI, audio playback, and mic/camera capture, all clocked to the render loop. *Accept:* a video frame at a fixed timestamp matches a golden via the software decoder; capture surfaces are stubbable and agent-observable.
*(✗: only deterministic stub models exist (`TestPattern`, `AudioBuffer::sine`, empty `CaptureSource`) — they remain the CI contract. **De-scoped post-2.0 per ADR-M3.**)*
**T6.4 ◐ Motion system.** Physics springs, gesture-driven interruptible animations, **shared-element transitions** across routes, and a choreography/timeline API; the inspector's scrubber becomes a keyframe editor. *Accept:* gesture-driven + shared-element transition tests are deterministic under the virtual clock; choreographed sequence golden.
*(◐→ M.3 ✅ wiring: `motion::shared_bounds(cx, name, target, ms)` — retained morph, animates on target change with smooth retargeting (route switches the hero's home ⇒ it glides); `motion::route_progress(cx, name, route, ms)` — 0→1 per `Router` navigation on the virtual clock; `motion::drag_surface`/`drag_fraction` — `on_drag` maps the pointer to a store fraction feeding `bounds_at_fraction` (the gesture IS the timeline). Keyframe evaluator shipped in B.5b. All store-backed (survive rebuild/snapshot), all clock-deterministic (tests/motion_m3.rs). Remaining for ☑: the choreography scrubber → keyframe-editor inspector story.)*
**T6.5 ◐ Advanced text & editing.** A real rich-text document model (styles, lists, tables, links, images), selection that spans widgets, find/replace, spell-check hooks, variable-font axis controls, and CRDT-ready edit hooks for future collaboration. *Accept:* rich-editor test triple; cross-widget selection + find/replace driven by the agent.
*(M.4 ✅: `richdoc::RichDoc` is the structured model — headings, paragraphs, bullet/numbered lists, `[text](url)` links (Role::Link, clickable), `![alt](src)` images (alt = accessible name), `**bold**`/`*italic*` spans; markdown-lite round-trip contract (`parse(to_source()) == doc`); find/replace over the source. `rich_text_editor` now edits the SOURCE with the full `TextEditor` caret/selection/clipboard/undo machinery (same engine as `TextField`) + a live parsed preview; `find_replace_bar` gives live match counts + replace-all. Explicitly *planned*: tables, spell-check, variable font axes (italics render muted-ink until then), CRDT.)*
**T6.6 ◐ Performance at scale.** Multi-threaded layout, on-device GPU damage/partial redraw, a memory profiler + leak gate, and CI enforcement of the remaining `01 §9` budgets (cold start <300 ms desktop / <800 ms mobile, hello-world <5 MB). *Accept:* a 100k-node scene + all `01 §9` budgets gated in CI on the reference runners.
*(◐: perf_gate (5 budgets incl. 100k cull) runs in CI. Multi-threaded layout **parked per ADR-R1** (backlog R4; virtualization is the answer); GPU damage scissor planned (plan R.1); memory/leak/cold-start gates **run in CI** (R.6 ✅ — headless cold start 2–3 ms vs the 300 ms budget, min-of-5; RSS-growth leak gate <32 MB over 300 frames; size gate FAILS now: default ≤24 MB regression guard + lean scaffold ≤8 MB, measured 6.8 MB with opt-z/LTO — the 01 §9 <5 MB target still needs a dependency diet). Size: `strip` landed (R.4); LN1 (2026-08-08) made both derived faces **byte-for-byte reproducible** — they had shipped as untracked pyftsubset output whose recipe existed only in a shell history, so the committed bytes could not be audited or re-cut. Ranges were recovered from the artifacts' own cmap coverage, verified to regenerate identical SHA-256s, and are gated by the `fonts` CI job (`scripts/subset_fonts.sh`). The T.4 font subset makes the lean profile real — hello release is 22.0 MB default (pan-Unicode face) / **7.5 MB lean**; the <5 MB budget needs the R.6 size gate against the lean profile.)* *(GX3, 2026-08-08: the **wasm lean profile did not exist** — `--no-default-features` gave 21.9 MB against a 22.0 MB default, because one `{ workspace = true }` link in a five-crate chain re-enabled the pan-Unicode face. Now **6.4 MB**, with a 4th web_gate leg detecting the regression. See 07 and `docs/constrained-profile.md`.)* *(CFG1/LN3, 2026-08-08 — corrected: the lean leg called `run_headless`, so LTO dropped the whole wgpu presentation path; same features windowed measure **13.3 MB**, not 6.8. A third **windowed** leg now gates at 16 MB. <5 MB is unreachable on desktop by any feature combination — `lumen-shell` depends on wgpu unconditionally because `Presenter` blits the CPU frame through a wgpu surface, so it needs a CPU presentation backend first. See `docs/constrained-profile.md`.)*
**M6-exit ◐:** a **media-rich, animated app** (video + SVG + shared-element navigation + a rich-text editor) holds 120 fps desktop / 60 fps mobile and passes every perf gate, agent-verified on desktop + both mobile emulators — added to the release gate.
*(◐: `agent-gauntlet-media` runs headless-desktop with the stub video source; frame budget is wall-clocked around `pump` (app.perf is stubbed); no mobile legs.)*

---

## M7 — Ecosystem, Production & AI-Native (ship it; advance the thesis)

*Theme: everything required to **ship, distribute, extend, and trust** a Lumen
app in production — then the AI-native frontier the project exists for: an agent
that doesn't just build UIs but **operates** them (repairs regressions, imports
designs, certifies a11y) autonomously. Culminates in the 2.0 release. New ADRs:
distribution/signing; plugin ABI; the ADR-014 hot-patching-linker tier-2 slot.*

**T7.1 ◐ Distribution & packaging.** `lumen package` → per-OS installers/bundles (msix/dmg/AppImage/apk/ipa), code signing + notarization, delta auto-update, an asset-optimization pipeline, reproducible builds, and binary-size + supply-chain (`cargo-deny`/SBOM) gates. *Accept:* signed, installable artifacts produced per platform; the agent triggers a versioned release end-to-end.
*(◐→ E.1 ✅ for the Linux leg: `lumen package` now resolves the WORKSPACE target dir + real version via cargo metadata, builds with `cargo auditable` when installed (dependency list embedded in the binary — verified via the `.dep-v0` section), writes a deterministic `sbom.json` (cargo-metadata packages, sorted/deduped) into the bundle, wraps the bundle as an `AppDir` (AppRun/.desktop/icon), and produces a runnable `.AppImage` (mksquashfs + cached type-2 runtime; degraded-to-AppDir offline). Live-verified: `counter.AppImage` executed and rendered. apk via script; cargo-deny in CI. Still blocked (CI-secrets/hardware, per D0.1): msix/dmg/ipa, signing/notarization, auto-update.)*
**T7.2 ◐ Plugin & widget ecosystem.** Third-party `Widget` distribution over a stable ABI; `lumen add <widget>`; a Storybook-class component gallery app (self-testing); semver-checked widget APIs; doc generation. *Accept:* an external widget crate is installed and driven by the agent unmodified; the gallery drives every widget through its own self-test.
*(E.2 ✅ for the 1.x story: **source-level `LeafWidget`/`Element` is the blessed plugin mechanism — a stable API, deliberately not an ABI** (ADR-W1; a dynamic ABI is a 2.x question). `lumen add <crate>` now resolves the REAL latest stable version from crates.io (offline degrades to `"*"` with a warning); "registering" a widget IS adding the dep and calling its constructor — no runtime registry by design. The out-of-repo-shaped `widget-rating` crate (public API only) is driven through the gallery and agent-gauntlet-2 self-tests. Still open for ☑: semver-checked widget APIs + doc generation.)*
**T7.3 ◐ Production hardening.** Error boundaries + panic recovery scoped to UI subtrees, crash/diagnostic reporting hooks, opt-in privacy-respecting telemetry, a security review, and fuzzing of the `.lss`/agent/asset parsers. *Accept:* an injected panic is contained to its subtree and reported as a structured diagnostic (app stays alive); parser fuzz gate green.
*(E.3 ✅ for hooks+fuzzing: `install_crash_hook` → structured `E0702` before the abort path (tested); libFuzzer targets for `.lss`/selector/agent-JSON/PNG+SVG under `fuzz/` (all smoke-ran clean on this box) + nightly `fuzz.yml`; bounded proptest fuzz-lite suites in every gate. Telemetry re-scoped: **explicitly not planned** (privacy stance, 01 §9b) — the "opt-in telemetry" wording above is superseded. Security review remains open.)*
**T7.4 ✗ Accessibility certification.** Real VoiceOver / NVDA / Orca driven in CI (not just AccessKit-tree diffs), a WCAG 2.2 AA audit with automated checks where possible, a11y of the inspector + agent themselves, and localized accessibility. *Accept:* screen-reader smoke tests pass in CI on 3 OSes; the WCAG checklist is automated where automatable and signed off where manual.
*(✗: no AT automation anywhere; `docs/a11y-checklist.md` itself marks the AT runner PENDING. The WCAG automated checks (contrast/name audits) exist and run headless. Depends on T4.3's adapter — plan P.4.)*
**T7.5 ◐ AI-native frontier.** An agent **auto-repair loop** (detect a regression → localize it via diagnostics + traces → patch → verify, unattended); the ADR-014 function-level hot-patching linker slotted in as an upgraded tier 2 (checkpoint protocol unchanged); design-import (Figma/Sketch → `.lss` + widgets) with agent reconciliation; self-describing components for agent authoring. *Accept:* the agent autonomously repairs an **injected functional regression** end-to-end with zero human edits; a design-import round-trips to a styled screen.
*(◐: the auto-repair loop is real and gated (agent-gauntlet-2, zero human edits). The hot-patching linker and design-import do not exist.)*
**M7-exit ◐ (2.0 release gate):** the grand gauntlet — an agent, given only the CLI + lumen-agent, **ships a complete production app** across all five platforms (desktop ×3 + web + mobile ×2): signed/notarized and installable, screen-reader-certified, localized (RTL+LTR), extended with a third-party plugin, with media + motion; it then **auto-repairs an injected regression** and re-ships — the entire pipeline green, zero human intervention, as `examples/agent-gauntlet-2/` and the 2.0 release gate.
*(◐: `agent-gauntlet-2` proves the headless slice (RTL, WCAG audits, plugin widget, auto-repair) on one OS; "signed/notarized", "screen-reader-certified", and the five-platform legs are not exercised — see T7.1/T7.4/T3.x/T5.1 notes.)*

---

# Appendix A — M0 Implementation Plan (agent working notes, non-normative)

These are my own working notes for executing M0. The normative contract is everything above plus docs 02–05; this appendix only records *how* I intend to satisfy it and *which order* I'll work in. Nothing here overrides a contract. Local decisions made here are also mirrored into `07-decision-log.md §3` as I land each task.

## A.0 Strategy & critical path

M0 is verification-first: T0.9 is the gate after which everything is golden-/semantics-testable. I work two tracks in parallel after the scaffold, converging at T0.9:

```
T0.1 scaffold+CI
   ├─ Core/interaction track:  T0.2 tree → ┬ T0.3 signals
   │                                        ├ T0.5 layout
   │                                        └ T0.7 events → T0.8 semantics
   └─ Rendering track:         T0.4 displaylist+CPU → T0.6 text
                                                    ↓
        T0.9 headless app + lumen-test seed  ◀── (needs T0.3–T0.8)   ★ gate
                                                    ↓
        T0.10 ten widgets → T0.11 winit+wgpu → T0.12 CLI → M0-exit
```

**Critical path** (longest dependency chain to M0-exit): `T0.1 → T0.2 → T0.7 → T0.8 → T0.9 → T0.10 → T0.11 → T0.12 → M0-exit`. T0.4/T0.6 (rendering) and T0.3/T0.5 must all be done before T0.9 but are not individually on the longest chain, so they're where parallelism buys time. **Do not** start T0.10 until T0.9's self-tests are green — widgets without the harness can't meet their "golden + semantics + interaction" DoD.

One PR per task, message prefixed `[T0.x]`, checkbox flipped in the merge commit (rule 8). Every task adds tests and rustdoc (rules 3, 10).

## A.1 Cross-cutting setup (decided once in T0.1, used everywhere)

- **Toolchain:** pin the current stable in `rust-toolchain.toml` (`channel = "stable"` + exact `x.y.z`), components `rustfmt, clippy`. Record the exact version in `07 §3` as MSRV. CI uses the pinned toolchain on all three OSes.
- **Workspace:** virtual manifest at root listing all 11 crates + `lumen` facade (`02 §1`), `examples/`, `benches/`. Lockstep version (`0.1.0` as of 2026-08-22 — minor bump for the F3.7 `impl Into<Text>` break; 0.x treats minor as the breaking slot), `publish = false` for now. The lockstep number is referenced by `just run`'s `-p <name>@<version>` member pins, which disambiguate the `image` EXAMPLE crate from the `image` dependency — bumping it means bumping those too. Shared `[workspace.dependencies]` table so every whitelisted crate version is pinned in exactly one place (satisfies ADR-003 "pin minor versions at repo init").
- **`RgbaImage` — local decision / watch item.** `02 §8` types `screenshot() -> RgbaImage`, but the `image` crate is **not** in the ADR-003 whitelist. Decision: define our own `lumen_render::RgbaImage { width, height, pixels: Vec<u8> /* RGBA8, row-major */ }` rather than pull `image`. PNG encode/decode for goldens uses tiny-skia's `png` feature (the `png` crate, already in tiny-skia's transitive closure) — encode via `Pixmap::encode_png`, decode via a thin `png`-crate reader. If `png` is judged outside the transitive closure, that's an ADR-003 escalation → `BLOCKED.md`. Re-export `RgbaImage` from the `lumen`/`lumen-test` facades so user/test code never names the internal crate.
- **Async executor — local decision.** `#[lumen::test]` bodies are `async`, but ADR-003 scopes `tokio` to "agent/dev-server only." Decision: the `lumen-test` macro wraps the body in a tiny hand-rolled single-threaded `block_on` (no waker threads; the headless app is synchronous via `pump`), keeping `tokio` out of the test harness. `resource()` futures (T0.3) are likewise polled cooperatively inside `pump`.
- **Golden infrastructure (built in T0.9, used by T0.10/T0.11):** helper in `lumen-test` that resolves `tests/golden/<renderer>/<name>[.<tag>].png`, does exact compare on CPU, writes `<name>.actual.png` + `<name>.diff.png` on mismatch, and re-records when `LUMEN_UPDATE_GOLDENS=1` (`05 §4`). CI never sets that env.
- **Diagnostics registry:** `lumen-core/diagnostics.md` seeded in T0.1 with every code from `02 §9` (W0001, W0002, E0101, E0102, W0103, E0201, W0301). A `Diagnostic` struct + `code: &'static str` consts land in T0.1 so later tasks only *emit* codes, never invent them (ADR-019).

## A.2 Per-task plan

### T0.1 — Workspace scaffold + CI
- Files: root `Cargo.toml` (virtual), 11 `crates/lumen-*/{Cargo.toml,src/lib.rs}`, `lumen/` facade, `rust-toolchain.toml`, `deny.toml` (MIT/Apache-2.0 allowlist per ADR-020), `.github/workflows/ci.yml`, `lumen-core/diagnostics.md`, per-crate `README.md` stub.
- CI matrix `{ubuntu, windows, macos}` × steps: `fmt --check`, `clippy --workspace -- -D warnings`, `build --workspace`, `test --workspace`, `cargo-deny check`.
- Each `lib.rs` compiles empty (or with the `Diagnostic` skeleton in core). Geometry re-exports from `kurbo`, `Color` type with `srgb8`/`from_hex` constructors land here (cheap, everything needs them).
- *Accept:* `cargo build --workspace && cargo clippy --workspace -- -D warnings` green ×3 OS.

### T0.2 — Node tree + SoA hot data (deps T0.1)
- In `lumen-core`: `NodeIndex { index:u32, generation:u32 }` + free-list allocator (generational reuse). Parallel arrays exactly per `02 §5` (`bounds, transform, opacity, clip, flags, z, parent, first_child, next_sibling`). `NodeFlags` via `bitflags`.
- Ops: `insert`, `remove` (recycle index, bump generation), `reparent`; iterators for **document order** (depth-first via intrusive links) and **z-order**; `hit_test(point)` as an array scan honoring `clip` + `HIT_TESTABLE`, highest-z-first then reverse document order (`02 §5`).
- *Accept:* `cargo test -p lumen-core tree::` with **proptest**: 10k random edits preserve invariants (no dangling indices, parent/child symmetry); hit-test matches a naive reference on 1k random scenes. → Write the naive reference impl in the test module first; it's the oracle.

### T0.3 — Signals + state store + checkpoint (deps T0.2)
- `signal/memo/effect/resource` (`02 §4`), Solid-style fine-grained (ADR-007). Keying = identity path folded with the key — any `Hash + Debug` value, not just a `&str` name (**ADR-021**, `docs/plan-hash-identity.md`, H0–H4 landed 2026-08-02). A subscriber graph maps signal→scopes; writes mark only subscribed scopes dirty and are **batched** per loop turn; effects run after rebuild, before paint.
- Store is the only retained mutable state; values are `Serialize + DeserializeOwned`. Snapshot = `serde_json`, field-tagged (ADR-011): missing fields → `Default`, unknown fields dropped + `W0002`. `Checkpoint { quiesce, serialize_state, restore_state, resume }`. `#[state_registry]` proc-macro for `Box<dyn StoredTrait>` (typetag-style, serialized by registry name).
- *Accept:* `cargo test -p lumen-core state::`: writing 1 of 10k signals re-runs exactly 1 scope (instrument a counter); 1k-signal snapshot/restore lossless; struct-evolution fixture (field add/remove) restores with defaults + emits W0002.
- *Risk:* the `#[state_registry]` macro and any public signal-API signature change are **escalation** (public API). Keep signatures verbatim from `02 §4`; if one won't compile, minimal fix + decision-log note.

### T0.4 — Display list + CPU renderer (deps T0.1)
- `lumen-render`: `DrawCmd` + `Brush` enums verbatim from `02 §7`. tiny-skia executor for rects/rrects/borders, paths (fill/stroke via tiny-skia, lyon reserved for GPU tessellation per ADR-006), 3 gradient kinds (interpolated in **Oklab**, ADR-017), images, layers (clip/opacity/transform/blend), damage-region rendering.
- **Bit-determinism** is the contract (ADR-002): no time-based dithering, fixed iteration order. Damage = union of dirty node bounds; re-render of dirty rect must equal full re-render cropped.
- *Accept:* `cargo test -p lumen-render`: per-command-class golden PNGs (exact); same scene twice byte-identical; damage crop test. Uses the golden helper — but that lives in T0.9, so T0.4 ships a *local* exact-PNG-compare helper and T0.9 later unifies it. (Note in PR.)

### T0.5 — Layout engine wrapper (deps T0.2)
- `lumen-layout` over **Taffy** (ADR-004), no taffy types in public API. Map the `04 §3` layout property set → Taffy style; incremental relayout of dirty subtrees; write results into SoA `bounds`. Wrapper owns baseline/intrinsic extensions.
- *Accept:* `cargo test -p lumen-layout`: 40-fixture suite (flex/grid/absolute/min-max/aspect-ratio) with exact bounds; dirty-subtree relayout touches only descendants (counted). Since `.lss` isn't parsed until M1, fixtures construct Taffy-mapped styles directly via the wrapper's typed input.

### T0.6 — Text v0 (deps T0.4)
- `lumen-text`: parley (shape/layout) + swash (scale/hint) wrapper (ADR-005). Single + multi-style runs, wrap, align, ellipsis. **Bundle Noto** (Sans/Sans CJK/Color Emoji) as the only test fonts — no system fonts in CI. CPU glyph atlas feeding `GlyphRun` draw cmds. Bidi + CJK fixtures from day one.
- *Accept:* `cargo test -p lumen-text`: goldens for latin/CJK/bidi/emoji/wrap/ellipsis; measurement returns stable sizes across runs.

### T0.7 — Event routing + focus (deps T0.2)
- `lumen-core`: `Event` enum verbatim `02 §6`. Capture (root→target) then bubble (target→root) using the SoA hit-test from T0.2; `Handled` stops bubbling. Pointer enter/leave tracking; `Tab`/`Shift+Tab` focus over `FOCUSABLE` in document order; `Timer` events. **One input queue** shared by OS + synthesized input (the single-path invariant tests/agent rely on).
- *Accept:* `cargo test -p lumen-core events::`: dispatch-order fixtures; enter/leave on synthetic moves; 20-node focus-ring order matches expected.

### T0.8 — Semantics tree + JSON export (deps T0.2, T0.7)
- `SemanticsNode` built during rebuild; elision of pure-layout nodes (splice children up); JSON schema **exactly** `03 §1`, with a JSON Schema file checked into the repo and validated in tests (dev-dep `jsonschema`). Selector engine = grammar `03 §2` (`#id .class role :state :text() :text-contains() :has() :nth() *`, descendant + `>`), runs over the **elided** tree in document order. W0301 for focusable leaf with no label/value.
- *Accept:* `cargo test -p lumen-core semantics::`: schema validation on fixtures; ≥30-case selector table incl. `:has`, `:nth`, ambiguity errors returning candidates.
- *Risk:* any field added to the schema beyond additive-optional is an **escalation** (doc 03). Implement exactly as specced.

### T0.9 — Headless app + harness seed ★ verification gate (deps T0.3–T0.8)
- `lumen-core`: `App::new/stylesheet/run_headless`, `Headless::{pump, inject, screenshot, semantics_json}` (`02 §8`). `pump` = drain input queue → rebuild dirty scopes → run effects → layout dirty subtrees → paint to display list → execute CPU renderer; returns `FrameStats`.
- `lumen-test` seed: `#[lumen::test]` macro (builds app from crate's `fn main_app() -> App`), `TestApp`, `Locator` (`click/fill/press/text`), `expect` (`to_exist/to_have_text`), **auto-wait** per `05 §3` (poll 10ms virtual-clock until single-match + visible + settled, else `Timeout`; `>1` → `Ambiguous` with candidates), exact-golden `expect_screenshot`, **virtual clock**. Unify the golden helper here (see A.1).
- *Accept:* `cargo test -p lumen-test` self-tests: auto-wait succeeds on delayed-appearance fixture; fails `Ambiguous` w/ candidates on duplicate fixture; golden round-trip; `LUMEN_UPDATE_GOLDENS` re-records.

### T0.10 — Ten primitive widgets (deps T0.9)
- `lumen-widgets`: Text, Image, Row, Column, Stack, Scroll, Button, TextFieldBasic (single-style, pre-IME), Checkbox, Slider (`02 §10`). Each implements build/layout/paint/event/`semantics()` (mandatory for leaves, ADR-009), keyboard map, **hardcoded** default styles (constants until T1.2), rustdoc + compiling example.
- *Accept:* per-widget triple — golden + semantic-tree + interaction (slider drag changes value; checkbox space toggles; scroll wheel moves content + updates `scroll` in semantics). `cargo test -p lumen-widgets`.

### T0.11 — winit shell + wgpu renderer (deps T0.4, T0.10)
- `lumen-shell`: winit window/surface, resize/scale, vsync present, damage-aware redraw. `lumen-render` GPU path (wgpu, ADR-001): glyph/image atlases on GPU, lyon path tessellation (ADR-006). **Parity harness**: GPU output vs CPU goldens at the perceptual threshold (`05 §4`: ΔE Oklab ≤2.0, ≤0.1% pixels differ).
- *Accept:* `cargo test -p lumen-render -- --ignored gpu_parity` on a GPU runner; `examples/hello` opens + renders the counter; idle CPU <0.5% over 10s (ignored test, desktop runner). GPU tests are `#[ignore]` by default (env assumptions in 00).

### T0.12 — CLI skeleton (deps T0.9, T0.11)
- `lumen-cli`: `lumen new` (scaffolds app exposing `main_app()`), `lumen run`, `lumen test` (wraps `cargo test`), all with `--json` output envelopes.
- *Accept:* integration test: `lumen new demo && cd demo && lumen test --json` passes and emits valid JSON.

### M0-exit
- `examples/hello` counter app; a CI lumen-test queries the tree, clicks `#increment` by selector, asserts label `1`, matches an exact golden — headless on Linux/Windows/macOS. This is just T0.9's harness + T0.10's Button/Text + T0.12's scaffold wired into one example; no new mechanism.

## TC1 ☑ Task cancellation (2026-08-10)
`cx.task`/`cx.resource` are owned by the declaring scope and cancelled when it leaves the view or a deps change supersedes them; `cx.abortable_task`/`abortable_task_blocking` return an `AbortHandle` for stopping one on demand. `Spawner::spawn`/`spawn_blocking` return `Box<dyn TaskHandle>`. *Accept met:* scope death, deps supersede, memo-skip survival, handler-driven abort, and restart-after-cancel all covered in `lumen-widgets/tests/data_layer.rs` + `examples/download_progress/tests/smoke.rs`; the evicted-slot panic has a regression test. *(Two pre-existing bugs fixed as prerequisites — the `scope_cache`-only dead-set in `sweep_dead_scopes`, and F5 × F1 memo-skipped parents orphaning child scopes (`scope_gc_nested.rs`). See 07 § TC1.)*

## TS1 ☑ Touch scrolling + click-on-release (2026-08-11)
A finger drag scrolls (`pan`, chained and clamped through the same wheel path), a flick coasts and decays, and — the piece that makes the first two usable — **`on_click` fires on the release, not the press** (02 §6). A press picks the target; the release activates it only if it lands back on that node, and a touch that travels past 10 px is a scroll, not a tap. Android's `MotionAction::Cancel` becomes a release marked `click_count: 0`, which ends the press without clicking or flinging. *Accept met:* `lumen-widgets/tests/{touch_pan,click_release}.rs` — 12 tests; each rule ablated individually and shown to fail without it. *Follow-up (2026-08-12):* rows built as bare `widgets::text` shrink-wrapped to their glyphs and were only tappable on the label — a percentage `width` was being overwritten by the measured run in text lowering (see 07). Fixed at the lowering site; `VirtualList` now fills `width: 100%` on an item that left it `Auto`. Covered by `list_item_width.rs` (3) plus a width assertion added to `lss_layout_properties::text_wrap_nowrap_keeps_the_run_on_one_line`, which had only ever checked the height. *Known gap:* the drag test's precondition (the pressed row rides the scroll and is under the finger at release) only holds when the harness pumps between moves, which is the device's shape; a batched-inject test passes vacuously.

## SR1 ☑ Surface-resize crash + text size honoured (2026-08-12)
A resize drag could abort the process: `present_to_surface` returned `bool`, the shell read a transient skip as "surface permanently gone", and the CPU-presenter fallback it then built configured a second wgpu surface mid-drag — fatal in wgpu 22. Now `Present::{Done,Skipped,Unavailable}`, and only the last falls back; `Presenter::new` re-queries the window size immediately before `configure`, clamps to the device limit, and is fallible. Same commit range fixes the 3 px dead strip between `VirtualList` text rows by honouring an explicit `height` on a text element (07). *Accept met:* live repro on this box — pre-fix died after 129 `wmctrl` resizes of `just run datagrid`, fixed build survived 1600; `present_outcome.rs` (3) pins the app-level skip/unavailable distinction; `list_item_width.rs` (5) covers row tiling and hit coverage. *Not closed:* `Surface::configure` racing a resize is fatal-by-construction in wgpu 22 and uncatchable from outside it.

## LW ☑ Live-window smoke gate (2026-08-13)
`just live-gate` (`scripts/live_window_gate.{sh,py}`) opens real windows on a real adapter and drives them through the agent RPC — the only automated thing in the repo that does. Seven legs, each named for the defect it would have caught (see `docs/live-window-gate.md`): boot, input (click vs drag), shadow-ink, diagnostics, multi-window, oversize, resize-storm. CI runs it under Xvfb **plus openbox** (an EWMH resize needs a WM to honour it), blocking, storm 120. *Accept met — both directions, not assumed:* with `e346f46` reverted and rebuilt, `resize-storm` fails with the original `Surface::configure: Invalid surface` panic; with `card.shadow = None`, `shadow-ink` fails. *Known gaps:* the secondary-window wheel inversion is NOT covered (no example opens a second window, so the leg only checks `ui.getWindows` answers), and CI is lavapipe, so no pixel-parity assertion lives here.

## WG ☑ wgpu 22 → 30 (2026-08-13)
Eight majors in one step, gated by LW (the live-window gate exists precisely because the upgrade's risk lives where no headless test reaches). No winit bump forced — wgpu 30 shares `raw-window-handle 0.6.2` with winit 0.30.13, and only our own three crates depend on wgpu. The acquire path is the substantive change: `CurrentSurfaceTexture` replaces `Result<_, SurfaceError>` and maps onto `Present::{Done,Skipped,Unavailable}` nearly one-for-one, with a new `Occluded` that is a clean skip. *Accept met:* 394 suites; GPU suite on native **and** lavapipe; `cpu_vs_gpu`; `just live-gate` all legs incl. a 400-resize storm with the direct path intact; size gate unchanged (7.6/22.1/6.9/13.6/10.5 MB). *Explicitly NOT fixed by this:* `Surface::configure` still returns `()`, so the fatal configure-vs-resize race remains uncatchable from outside wgpu (see 07).

## EX ☑ Executor adapters — tokio + smol (2026-08-13)
`lumen-exec`: a leaf crate with `tokio` / `smol` behind default-off features, surfaced as `lumen::exec` under `lumen/exec-tokio` / `exec-smol`. Fixes the real limit of `ThreadPoolSpawner`, which `block_on`s a future on a pool thread — capped concurrency, cooperative-only cancellation, and reactor-dependent futures that **panic** rather than run. Kept out of `lumen-core` deliberately (GX3/CFG1 feature-unification trap); verified the default `cargo tree -p lumen` has zero tokio/smol nodes. *Accept met:* `tests/reactor.rs` (the same timer completes under tokio, never on the thread pool), `tests/cancellation.rs` (three spawners; strong abort asserted only where it holds), `tests/in_app.rs` (full round trip into a signal). *Found on the way:* smol's `Task` cancels on drop, violating `Spawner`'s contract — now detaches on drop, cancels only on abort. **No HTTP**, by product decision: transport is the app's.

## CP5.1 ☑ Memo-hit lowering measured (2026-08-13)
The measurement the CP5 gate owed the record; nothing shipped. Re-lowering an unchanged span (`copy_span`) is **33.8%** of a memo-hit frame, so a retained graph that removed it all would take `scoped_vs_flat` from **0.648** (re-measured; the recorded 0.787 predates OB2 and the `link_last_child` quadratic fix) to **~0.43** — the live side of the gate's own 0.49 line. *The finding that matters:* taffy node construction is only ~18% of that cost; ~82% is the tree rebuild plus moving `NodeMeta`/`node_style`/`node_computed` between hash maps. **CP6 as written ("persisting the arenas") therefore buys ~6% of a frame, not the ceiling** — re-gate the larger version or expect a sixth of the win. Says nothing about the egui ratio (BENCH1 has no `cx.scope`); CP4 still missing. Full method + the instrumentation-overhead correction in `docs/cp5.1-memo-hit-lowering.md`.

## CP6 ☑ Re-gated on the retained-TREE version — STOP (2026-08-13)
Run at the owner's request against the bigger scope CP5.1 identified (retained tree + side tables + taffy), which the 2026-08-08 gate had explicitly left open as CP6.2. **The number clears every bar** — full retention removes 20.2% (attributed) to 33.8% (whole copy path) of a memo-hit frame, taking `scoped_vs_flat` from 0.648 to 0.43–0.52. **Stopped anyway, on a different input:** building the tree costs ~0 ns/node (it matters only as the enabler that stops side-table indices moving, 13.8%; taffy is 6.3%), and **0 of 51 example crates, 0 shipped widgets and 0 non-test call sites use `cx.scope`** — CP6 would speed up a path exercised by one test and one bench. *Successor is work, not another measurement:* **ADOPT** — make `VirtualList`/virtual table/`DataGrid` memoize their rows, which is reversible, needs no machinery, and buys 1.54× on its own. Re-gate CP6 after adoption, against list workloads, and with the still-missing ARM number. Ruling: `docs/cp6-retained-tree-gate.md`.

## ADOPT ◐ Memoized list rows — shipped, mostly does not pay (2026-08-13)
The CP6 gate's successor, built and measured. **Two discoveries.** (1) Memoizing a row is *unsound* without a caller-supplied dep: `cx.scope` invalidates on signals READ, and the usual list shape reads none (the parent read the signal and the row captured the value), so an empty `ReadSet` is always current and the row freezes forever. Added `cx.scope_with_deps(id, deps, f)` — deps beside the key, not folded into it, so a change re-runs without shedding scope-local state. (2) The gate's predicted **1.54× does not transfer**: it came from a *non-virtualized* 500-row list, and every widget ADOPT targeted is virtualized. Measured on a 100k-item `VirtualList`, one row changing: **1.01×** at 1 element/row, ~1.1× at 16 — virtualization already captures what memoization would. *Shipped:* `cx.scope_with_deps` (fixes real unsoundness, `scope_deps.rs`), `VirtualList::memoized` (opt-in, doc leads with the 1.01×, `virtual_list_memo.rs`). *Not shipped:* a perf-gate ratio for a win that isn't there; `DataGrid`/virtual-table memoization (their rows are framework-built from `cell_text`, so a dep would call it anyway — unmeasured, deliberately). *Method note worth keeping:* the first measurement said 1.49× across three consistent runs and was a process-warm-up artifact — swapping the arm order inverted it exactly. Reproducibility did not catch it; the order swap did.

## PG ☑ Pre-push CI gate (2026-08-17)
`scripts/ci_local.sh` mirrors every `ci.yml` job this box can run, as twelve named legs across two tiers; `.githooks/pre-push` (tracked, installed by `just install-hooks` setting `core.hooksPath`) runs the fast tier — **139 s** — before a push leaves the machine. Each leg prints the CI job it stands in for, and the run ends by listing what it did **not** cover, because a green subset that reads as a green matrix is worse than no gate. *Why now:* the push of 2026-08-12 went red on four jobs at once (clippy, lean, cold-start, gpu), all reproducible here in minutes, while `just check` covered only three of the nine jobs and no gate ran `lean`, `gpu`, `executors`, `fonts`, `perf` or `live-window`. *Accept met — both directions:* an injected `unused_variables` fails the clippy leg with exit 1; the deletion-only, `--no-verify`, `LUMEN_PREPUSH=off` and dirty-tree branches were each exercised. **All twelve legs run green on this box**, measured after a `cargo clean` so the numbers are cold: fast tier 242 s cold / 139 s warm (fmt 0 · clippy 22 · test 127 · doc 5 · lean 71 · executors 16), then gpu 24 · fuzz 71 · live 35 · perf 528. `deny` and `fonts` report SKIP here for missing local prerequisites (cargo-deny 0.18.3 predates CVSS 4.0; no fontTools), which is why SKIP is a distinct outcome rather than a pass. *Found on its first run, on a tree believed clean:* `lumen-app` failed the `lean` leg — `Present` was imported unconditionally but used only under `cfg(feature = "wgpu")`, committed in SR1/WG and queued to break the next push. Exactly the LN0 class the job was created for. *Deliberate non-faithfulness:* the gate shares `target/` (this box is at 85 % disk with a 120 GB tree, so a second one will not fit) and therefore does **not** export `RUSTFLAGS=-D warnings` globally — `cargo clippy --workspace --all-targets -- -D warnings` covers the same target set without forking every dependency fingerprint. The `lean` leg does export it, since that job has no clippy step. *Capacity, measured the hard way:* one `cargo build --workspace --all-targets` writes **~25 GB** to `target/debug` (51 example crates × every target kind), and this box ran out mid-run twice — surfacing as `couldn't create a temp dir` and `failed to build archive`, which read like code errors and cost an hour chasing a phantom flaky test. The gate now refuses to start under 30 GB free (`LUMEN_CI_MIN_FREE_GB`) and prints what to reclaim. *Known gaps:* the Windows/macOS matrix legs are unreachable from Linux, and **the nightly fuzz job is not push-triggered and cannot be predicted by any pre-push gate** — the `fuzz` leg replays the committed corpus for regressions only (see `docs/fuzz-selector-has-blowup.md`).

## DEP1 ☑ Dependency audit — and why CI had been red for a month (2026-08-17)
Prompted by "anything else needing a bump besides wgpu?". 15 crates were a major series behind; the audit's finding was not any of them. **CI had not been green in its last 8 runs**, and the current cause was a missing system package: `lumen-shell` pulls gtk 0.18 transitively (rfd/muda/tray-icon, ADR-P1) since mid-July, no job installed `libgtk-3-dev`, and every job that compiles the shell died on `The system library gobject-2.0 ... was not found`. In `lint` that killed clippy — which runs *before* cargo-deny in the same job, so **cargo-deny had never actually executed**, and was hiding a vulnerability and a license rejection when it finally did. *Landed:* (1) `crossbeam-epoch 0.9.18 → 0.9.20`, closing RUSTSEC-2026-0204 — a lockfile bump, no dependency change; (2) `libgtk-3-dev` in `lint`/`test`(Linux only)/`lean`/`perf`/`live-window`, and a note above `jobs:` recording which jobs do *not* need it and why; (3) `BSL-1.0` allowlisted in `deny.toml` (clipboard-win + error-code, Windows-only via arboard — permissive, OSI-approved); (4) `accesskit 0.17 → 0.24` + `accesskit_winit 0.23 → 0.33`, which move together and need **no winit bump** (0.33 accepts `winit ^0.30.5`). *Four API breaks, all small:* `Tree::app_name` deleted — safe to drop, because `accesskit_unix` now derives it from `current_exe()` itself, which is P.4's hand-rolled workaround upstreamed; `TreeUpdate.tree_id` added (we publish `TreeId::ROOT`, grafts unused); `ActionRequest::target` split into `target_tree`/`target_node`. *Paid for itself downstream:* the bump removed `quick-xml 0.30` (two DoS advisories) and `paste`, and unpinned `parley` to 0.11.1 — accesskit 0.17 had been silently holding parley back, since 0.11.1 requires accesskit 0.24. *`deny` is green for the first time,* with a documented `ignore` list: the seven gtk-rs GTK3 EOL advisories (an ADR-P1 architecture decision, no maintained GTK3 binding exists), `proc-macro-error` (rides with GTK3), `ttf-parser` (winit's Wayland CSD path, not Lumen's text stack), and `quick-xml 0.39` ×2 — **not reachable**: its only consumer is `wayland-scanner`, a proc-macro parsing vendored protocol XML at build time. *Accept met:* all twelve gate legs green, goldens byte-identical through the parley bump, `live-gate` all 7 legs on the new adapter (which panics if the window is already visible, so the invisible→attach→show order is genuinely exercised), and size gates **7.6 / 22.1 / 6.8 / 13.7 / 10.6 MB** — unchanged from the wgpu-30 baseline, so zbus 4 → 5 cost nothing. *Method note:* the perf gate's first run after `cargo clean` reported `scope_scaling_300_over_50` at **1.665** against a 1.55 ceiling; warm re-runs give 1.380/1.387/1.404. A cold first run is not a measurement — the same warm-up artifact that faked ADOPT's 1.49×. *Not done, deliberately:* taffy 0.7 → 0.13 (six majors of layout API; pair it with the F2 retained-layout question), kurbo 0.11 → 0.13 (Lumen-only, so self-contained, but it is the geometry type in every public signature), tiny-skia 0.11 → 0.12 (the golden reference renderer — ADR-002 determinism is the risk), and the GTK3 exit itself.

## DEP2 ☑ taffy / kurbo / tiny-skia (2026-08-18)
The three bumps DEP1 deferred, each landed separately so a golden shift is attributable to one crate.

**kurbo 0.11 → 0.13 ☑.** Zero source changes — `Point`/`Size`/`Rect`/`Affine`/`Vec2` are unchanged across both majors; the churn was elsewhere (0.13 adds a `polycool` dep for path work Lumen does not use). All twelve legs green, every golden byte-identical. **It is still a breaking change for consumers:** `lumen_core::geometry` re-exports the kurbo types (`geometry.rs:8`), so an app on kurbo 0.11 gets type mismatches against Lumen 0.13. That is not theoretical — `fuzz/` is excluded from the workspace and carries its own lockfile, so it stayed on 0.11 and failed to compile against `run_headless(Size)` exactly as a downstream app would. The `fuzz` leg caught it; the nightly job would have been the alternative discoverer, a day later. Its pin now carries a comment saying it must track the workspace.

**tiny-skia 0.11 → 0.12 ☑.** One API break, and it is a semantic one worth stating: `RadialGradient::new` moved from `(start, end, radius)` — Skia's `MakeTwoPointConical` with the START circle's radius implicitly 0 — to the full two-circle `(start_point, start_radius, end_point, end_radius)`. Lumen's radial is concentric, so the exact translation is the same centre twice with radii `0 → radius`; `golden_gradient_radial` confirms it is pixel-identical, which is the only acceptable evidence for the crate that IS the ADR-002 reference renderer. Every other golden byte-identical too. **tiny-skia 0.11.4 stays in the graph** — winit → sctk-adwaita (Wayland client-side decorations) pins it, so both majors are linked; `deny.toml` has `multiple-versions = "warn"`, and the size gate shows the cost: 7.7 / 22.1 / 6.8 / 13.9 / 10.9 MB against the 7.6 / 22.1 / 6.8 / 13.7 / 10.6 baseline, i.e. +0.1 to +0.3 MB, all still inside budget. Also removed `png 0.17` (0.12 uses 0.18), deduplicating png.

**taffy 0.7 → 0.13 ☑.** Six majors, **two edits**. (1) `AlignItems`/`AlignContent` became structs of `{ keyword, safety }` with the old variants as associated CONSTS — `Start` → `START`. Every const carries `AlignmentSafety::Unsafe`, which is CSS's default and what 0.7 did, so the rename is the whole change; Lumen deliberately does **not** adopt the new `SAFE_*` overflow behaviour, which would be a layout change rather than a port. (2) a grid template entry is now a `GridTemplateComponent`, either one track or a `repeat()` group — Lumen's `GridTrack` has no repetition form, so every entry wraps as `Single`. Nothing else in `lumen-layout` (693 lines across three files) moved. *Accept met:* all twelve legs green, **every layout golden byte-identical** — the result that actually matters, since six majors of a layout engine is precisely where geometry drifts silently. `layout_10k_dirty_subtree` 0.434 ms against a 2.0 ms budget (0.380 on 0.7; the gap is within the spread seen across runs today and nowhere near the budget). Sizes 7.7 / 22.2 / 6.9 / 14.0 / 10.9 MB. Dropped the `grid` crate from the graph. *Cumulative across DEP2's three bumps:* +0.1 / +0.1 / +0.1 / +0.3 / +0.3 MB against the DEP1 baseline, entirely from tiny-skia 0.11 and 0.12 both linking (winit pins the old one).

## TG1 ☑ Consensus test-gap sweep (2026-08-18)
Three independent read-only audits swept for untested behaviour from different entry points (public API; invariants/lifecycle; change history + platform seams), each required to disprove its own findings against the 733 existing tests before reporting. Only gaps **two or more** of them reached independently were acted on — 5 of 52 raw findings — and each was re-verified against the source by hand before any code was written. Every test below was **ablated**: the behaviour was broken deliberately and the test shown to fail with the predicted message. *Three of the five turned out to be defects, not merely untested code.* **(1) `tree.rs` `last_child`** — pure test gap: `check_invariants` validated `parent`/`first_child`/`next_sibling` and silently omitted the tail cache, so a stale-but-live pointer produced a correct document order while the next append went to the wrong node. Now liveness-checked *and* compared against a walk of the sibling chain; the existing 1024-case proptest does the rest. **(2) `Scrollable` shrink clamp** — pure test gap: every existing scroll test used a constant `content_h`, so the documented clamp was never reached; ablation renders the list blank at `y0=-700`. **(3) `asset` caches** — **two real bugs**: `png()` capped the shared cache and `decode()` (the jpeg/gif/webp entry point) did not, so CACHE1's unbounded growth was still live on every format except PNG; and `clear_cache()` — what the iOS memory-warning and Android LowMemory handlers call — left `ANIM_CACHE` resident, the largest thing in the process. Policy is now single-sourced in `insert_capped`, animations are capped separately (an entry is N frames, not one), and `anim_cache_len()` makes it observable. **(4) agent auth** — **a real bypass**: see 03 §Auth. **(5) SVG nesting** — **a real DoS**: `parse_tree` is iterative but `walk`/`collect_defs`/`Node::drop` recurse, and SVG is app-supplied data. Measured: 256 levels rendered, **512 aborted the process** — a stack overflow is `SIGABRT`, so `catch_unwind` cannot contain it and the error boundary does not protect the window. Depth capped at 64 during parse, which bounds all three recursions at once; over-deep content is dropped, not misrendered, and its closing tags are balanced so shallow siblings survive. **Then reviewed by two more agents, and they earned their keep — 4 of the 14 new tests were weak or vacuous, and one fix was a regression.** *Corrected in the same pass:* the SVG cap's first design used a counter for "opens I skipped", which broke the parser's advertised fault tolerance — a closing tag cannot tell "closes something I dropped" from "closes something never opened", so `<svg>` + 70 `<g>` + `</svg>` + `<rect/>` left the counter skewed and silently swallowed the rect (1 cmd before, 0 after — verified, then fixed by bounding ATTACHMENT instead of the parse stack, which leaves push/pop byte-identical to the pre-cap parser and recovers as soon as depth drops); `growing_the_content_again_restores_scrollability` passed with the clamp deleted, because it set the offset to a value the final extent made legal — replaced with a shrink-then-grow that pins the clamp as **render-only** and fails if it is ever written back; `animation_cache_stays_bounded` used 40 inserts against a cap of 8, a divisibility coincidence equally satisfied by 16 and 32 — now `cap*3+1` so only one cap gives the asserted length; `MAX_CACHED_ANIMATIONS = 8` was a **CPU cliff**, since `animated()` re-enters every frame edge and eviction is whole-cache, so 9 on-screen GIFs re-decoded every frame — raised to 32 with the limitation stated rather than hidden; `decode_honours_the_cap` fed PNG bytes while claiming to cover the jpeg/webp path *and* was gated on the one feature that path does not use — ungated, with a second test on the real codec path. *Also from review:* an **empty** `LUMEN_AGENT_TOKEN` counted as configured (`env::var` returns `Ok("")`, and the guard tested `is_err()`), so `LUMEN_AGENT_ADDR=0.0.0.0:9230 LUMEN_AGENT_TOKEN=` published the exact tokenless socket C.5 exists to prevent — both call sites now go through one `normalize_token`; the address is trimmed before parsing (a trailing newline used to fail closed); the token compare is constant-time; the proptest seed was replaced by a named test that says what it means; the depth boundary is pinned at 62/63 so the cap cannot be silently raised; and **no CI job compiled the `agent` feature at all**, so the guards' call sites were uncovered — `executors` now builds `lumen-shell --features agent` (and therefore needed GTK headers added). *Known and NOT closed:* the same shrink clamp is hand-written twice more in `lists.rs` (`VirtualList` and the virtual table) and only `Scrollable` is covered — the identical "policy duplicated per site" shape this task fixed for `asset.rs`. *Not acted on:* 47 single-auditor findings, and the `:has()` selector blowup (already measured and documented in `docs/fuzz-selector-has-blowup.md`; a test belongs with the fix, not before it).

## BENCH2 ☑ Competitive measurement vs iced / Xilem / GTK3 / Flutter (2026-08-19)
BENCH1's "Next" asked for exactly this. Two axes, because only one can include a non-Rust framework: in-process **frame cost** (Rust only, matched stopping points) and **whole app** (size + idle memory, everyone). Full numbers and caveats in `docs/results-competitive-2026-08-19.md`. **The result that matters:** against iced — retained and reactive, so a much closer peer than egui — Lumen is **7.2–8.3× slower in the steady state**, flat across a 30× size range. But with **every** row's text changed so neither side's text cache can help, the two **converge**, and at 3000 rows Lumen is **6% faster** (16.66 ms vs 17.76 ms). Stated as one line: iced's paragraph cache buys it **54×**, Lumen's buys **6×**. The pipeline — build, reconcile, taffy, paint, semantics — is already competitive; the deficit is text caching in the state an app actually lives in. That is the same conclusion BENCH1 drew from egui, now confirmed against a second framework and quantified instead of inferred. **Idle memory is Lumen's best axis anywhere so far:** lightest GPU framework measured (157.8 MB vs iced 191.9, Flutter 203.9, Xilem 320.2), and the no-GPU softbuffer profile at **11.6 MB is 2.5× lighter than a minimal C GTK3 app** (29.5 MB). **Size is its worst:** 13.5 MB windowed against iced's 8.4 and Xilem's 9.7 (Flutter's bundle is 22 MB; GTK3 is a 14 KB binary against 30.8 MB of shared libraries). *Harness fairness, twice:* `iced_core`'s null renderer is `impl Renderer for ()` with `type Paragraph = ()`, which makes text shaping a NO-OP — using it would have compared Lumen's parley/swash against iced doing nothing, the same class of error BENCH1 corrected in the other direction; `iced_tiny_skia` is used instead and two sanity tests assert iced really shapes and really lays out every row. *And a discarded run:* the first full benchmark executed while three builds and a series of GPU window launches were in flight, reporting iced 43% slower at 1000 rows and a phantom 2.2 ms Lumen "cliff" at 1400; criterion's confidence intervals were tight in both runs and did not reveal it. Only the idle-machine run is published. *Blocked:* GTK4 (`sudo apt install libgtk-4-dev`); *not done:* Xilem frame cost (masonry's `TestHarness` is the way in), Flutter frame cost (Dart engine — no in-process equivalent), Slint.

## BENCH3 ☑ GTK4, masonry and Slint added — and two BENCH2 claims corrected (2026-08-19)
Closes what BENCH2 left blocked. `docs/results-competitive-bench3.md`. **The new frame-cost number:** against **masonry** — Xilem's widget layer, measured through its own `TestHarness` with `SKIP_RENDER_TESTS` so it stops at the vello `Scene` + AccessKit update, the direct counterpart of Lumen's display list + semantics tree — Lumen is **2.8–3.5× behind**, against iced's 7.2–8.3×. masonry is the first opponent that also maintains an **accessibility tree every frame**, and the gap more than halves; that does not isolate the variable, but it is the first evidence the semantics-tree caveat was carrying real weight rather than excusing. Caveat stated in the report: masonry is Xilem's *lower half*, so this understates a full Xilem frame. **Two BENCH2 claims corrected.** (1) "Lightest GPU framework" was too strong — **Slint idles at 66.9 MB** against Lumen's 157.8; the defensible claim is lightest of the *wgpu-based* frameworks, since Slint's default femtovg/GL backend is a cheaper stack. (2) The GTK3 row does not stand for "GTK": **GTK4 idles at 133.9 MB against GTK3's 29.6**, 4.5×, because GSK renders through the GPU. **What survives unchanged and got stronger:** Lumen's softbuffer profile at **11.6 MB is 2.5× lighter than GTK3 and 11× lighter than GTK4** — the best whole-app result in any of the three reports, and not a GPU comparison, so the correction does not touch it. Sizes: iced 8.4, Xilem 9.7, Lumen-noGPU 10.6, Slint 12.2, Lumen-wgpu 13.5, Flutter 22 MB; GTK is a 14 KB binary against 30.8 (GTK3) / 35.1 (GTK4) MB of shared libraries. *Slint has no frame-cost row, deliberately:* `i-slint-backend-testing`'s `TestingWindow` leaves `renderer: None` unless one is named, so by default it **never paints** and timing it would measure strictly less than Lumen; naming one rasterizes into a buffer, which is strictly more, and its embedded partial-redraw design is structurally different again. Both options produce a number that looks comparable and is not. The way in is a third stopping point — full CPU frame *including* raster, with Lumen measured the same way — recorded as the next step. *Method note:* Lumen's own 3000-row figure moved 13% between two idle-machine runs (3086.8 vs 2736.2 µs), so these ratios are good to about one significant figure.

## BENCH3.1 ☑ Three corrections, one of them to a premise both reports shared (2026-08-19)
Prompted by four questions on BENCH3, three of which found real defects. **(1) Lumen does NOT rebuild the semantics tree every frame.** `sem_root()` is lazy — OB2, recorded as unlanded in BENCH1, has since landed — and `pump()` never calls it; every caller is a query path. So **every frame number in BENCH2 and BENCH3 was measured with no accessibility tree being built**, and the "unfair to Lumen" caveat both leaned on was false. Measured by forcing one per frame: the tree costs **12–18%** (2672 → 3027 µs at 3000 rows), which could not have explained a gap differing 2.5× between opponents anyway. The real reason Lumen's masonry ratio (3×) is smaller than its iced ratio (8×) is arithmetic: **masonry is itself 2.7× slower than iced** (874 vs 328 µs at 3000). Two reports reasoned from an unmeasured premise. **(2) GTK4 frame cost is now measured, and only half of it is measurable.** Layout scales linearly (`gtk_widget_measure`, 8.8 µs at 100 rows → 123.2 µs at 3000). Paint could not be added because **GTK culls**: `GtkWidgetPaintable` yields a `GskRenderNode` — the direct counterpart of Lumen's display list — but its bounds are 40×796 at 100 rows and 48×800 at 1000, i.e. only the visible rows. Lumen, iced and masonry all paint every row, so the same benchmark would ask GTK for ~50 rows of work and the others for 3000. *The first version of that harness produced sub-microsecond, perfectly FLAT timings across a 30× range* — the same "opponent does nothing" signature as iced's null renderer, caused by `gdk_paintable_snapshot` returning NULL on an unrooted widget. It would have published as "GTK is 4000× faster than Lumen". **(3) Slint's 66.7 MB verified as real.** It is genuinely on a GPU renderer (81 GL/EGL mappings), and per-process `nvidia-smi` shows GPU-side memory is 4 MiB for Slint, 7 for GTK4, 19 for Lumen — nowhere near enough to shift the ordering, so RSS is capturing essentially everything. `VmSize` differs wildly (iced reserves 3.4 GB of address space against Lumen's 1.4) and is reservation, not use. *Ops note:* while cleaning up what looked like a stray process from the live-window gate, a `pkill -x win_seeded` killed the user's running Mercurium dev window — the name collided and the process was not checked before killing it.

## PROF1 ☑ Where the iced gap actually is — and BENCH2's answer was the wrong half (2026-08-19)
`docs/profile-vs-iced-2026-08-19.md`. No sampling profiler on this box (`perf_event_paranoid=4`, no valgrind), so: framework counters + subtractive view variants + standalone pricing of the hot path's primitives. **Finding 1:** a one-row change rebuilds **100% of nodes** (3001 rebuilt / 0 copied) — `copy_span` is only considered for a memo-hit stub (`Element::shared`), which only `cx.scope` produces, and a plain view has none. iced needs no equivalent opt-in: its widget `Tree` persists and each `Text`'s shaped `Paragraph` lives in `tree.state`. **Finding 2, the central one:** engaging `cx.scope_with_deps` per row takes the frame from 3001 rebuilt to **2 rebuilt / 2999 copied** — and buys **10%** (2655 → 2377 µs). `copy_node` does not reuse a node, it **re-materialises** one: fresh `NodeIndex`, **fresh taffy node**, nine side tables re-keyed. Memoization skips only the view closure and style resolution. **Finding 3, attribution of the 0.516 µs/node no-content floor:** taffy `new_leaf` 0.108 (21%), taffy `compute_layout` 0.100 (19%), 9 FxHash ops 0.046 (9%), `Element` construct/drop 0.029 (6%) — **55% accounted, 45% not** (arena, `NodeMeta`, style memo, paint emission), which needs perf or in-crate timers and is recorded as open rather than guessed. **Taffy is 40% of the floor and `new_leaf` alone ≈ iced's ENTIRE per-row cost** (0.108 vs 0.107 µs). **BENCH1's hypothesis is retired:** the "8 hashmap ops per memo hit" it blamed are 9% of the floor and under 5% of the frame. Text is 41% of the frame but only 5% is layout measurement, and BENCH2's cache-denied run already showed the text pipeline at parity — so it is explicitly the wrong target. **Recommendations, ordered, and the ordering is the point:** R1 stop re-minting taffy nodes on the copy path (~21% of the floor, contained, a strict subset of R2); R2 reopen **F2 retained node graph / stable `NodeIndex`** — the only change that reaches iced's *shape* rather than a fraction of its constant; R3 dense side arrays for the `NodeIndex`-keyed maps (~9%, mechanical); R4 automatic memoization **only after R1/R2**, since it is worth 10% today and would spend the API budget for a tenth of the win; R5 do **not** optimise text.

## PROF1.1 ☑ The missing 45%, found — and the text cache costs 14% to LOOK UP (2026-08-20)
`perf_event_paranoid` lowered, so PROF1's unattributed 45% is now attributed. **Method corrections first, because two changed numbers:** a first pass with `--call-graph dwarf,16384` put 54% of the frame in the kernel — that was mostly the profiler copying 16 KB of stack per sample; flat, the kernel is **16%**. And this box is a hybrid i9-13900KF, so unpinned measurements varied ±40%; `taskset -c 2` brings it to ±1%. **Bucket 1 — memmove, 22.8%**, and with frame-pointer call graphs all of it resolves to *containers regrown from empty every frame*: taffy slotmap `grow_one`→`realloc` 7.9%, `leaf_ref`'s `Style` copy 4.8%, `hashbrown::insert` of large values 5.6%, `Tree::alloc` growth 1.1%. Confirmed in source — `rebuild_inner` does `let mut layout = P::Layout::default()` (`app.rs:3257`), so taffy's slotmap grows 0→3001 per frame. **Bucket 2 — the shape cache costs ~14.3% to look up:** `shape_cache` is `HashMap::new()`, i.e. **SipHash**; `ShapeKey` owns a `String` of the full text and `ShapeKey::new` does `text.to_string()`, so each lookup allocates the key it is about to hash, then `PartialEq` compares the whole string on a hit — sip 5.4 + `eq` 4.3 + `memcmp` 2.9 + hash/new 1.2. **Lumen spends 14% of a frame looking up a cache to avoid work it then does not do; iced spends zero, its paragraph being a pointer deref in the widget's own state.** *Validated:* swapping that one hasher to FxHash and re-measuring pinned gave **2467 → 2361 µs, 4.4% from one line** (reverted; it also needs `sweep` generic over the hasher). **Corrections to PROF1:** "text measurement is 5%" was noise — re-measured properly, the variant that removes it is *slower*; "the hashmaps are not the problem" is half wrong — the per-node maps are minor but the shape cache's hashing plus key comparison is the second-largest bucket in the frame. **Revised recommendations:** R1 FxHash the shape cache (measured 4.4%); R2 stop comparing whole `ShapeKey`s (7.2%); R3 reserve/reuse the taffy tree (12.7%); R4 reserve/reuse the per-node maps (5.6%); R5 store the shaped block per node, iced's model (removes bucket 2); R6 F2 retained node graph, of which R3/R4 are subsets. R1–R4 are contained, individually testable, and together address ~30% of the frame. `floorf` at 1.7% is a libm call, but `target-feature=+sse4.1` lost in noise and is not recommended.

## R1 ☑ FxHash for the text caches (2026-08-20)
PROF1's first recommendation, and the only one that arrived with a measured number rather than an estimate. `lumen-text`'s `shape_cache`, `run_cache` and `GLYPH_CACHE` were `HashMap::new()` — std's **SipHash-1-3**, which is DoS-resistance bought for keys the app itself mints and re-probes every frame. `sip::Hasher::write` was **5.4%** of a 3000-row frame. *Measured:* 2467 → 2337 µs, **5.1%**, pinned to a P-core, three runs each side (the pre-measurement predicted 4.4% from the shape cache alone; converting the run and glyph caches too accounts for the rest). *Structural note:* the hasher moved from `lumen-app`'s private `fxhash` module to `lumen-core` as public, rather than being copied — a second copy is the "policy written out per site" shape that let `asset::decode` miss the cap `png()` had (TG1). `lumen-app` re-exports it under the old path, so its ~35 call sites are untouched. `sweep` became generic over the `BuildHasher`. Safe because ADR-021's `IdHasher` is untouched and none of these caches' iteration order reaches a serialized value; goldens (CPU, GPU and the i18n/CJK set) are byte-identical.

## R2 ☑ ShapeKey is a 128-bit content hash, not the content (2026-08-20)
`ShapeKey` owned the full `String` plus three `Option<String>`s, so every lookup allocated the key it was about to hash (`text.to_string()`) and every HIT compared the whole string again. Now the key *is* the hash: lookups allocate nothing, `eq` compares 16 bytes, and the map hashes 16 bytes instead of the text. Collision policy is ADR-021's and the argument is deliberately the same — 128 bits chosen so no probe is needed; at the cache's 16 384-entry hard cap the birthday probability is ~1e-30, and a collision would render one string with another's shaping, which is strictly less severe than the snapshot corruption ADR-021 accepts the same risk for. *Measured:* **~1%** on its own, not the 7.2% PROF1 estimated — that estimate counted `eq` + `memcmp` as removable without accounting for the hashing added back, since any content-keyed cache must still hash the content once. **Together R1+R2 are what matters:** the text-cache bucket went **14.3% → 5.4%** of a 3000-row frame (`sip::Hasher`, `ShapeKey::eq` and `__memcmp_avx2` have left the profile entirely), and the frame is **2467 → 2323 µs, 5.8%**. The residue is `TextEngine::shaped` 3.7% and `ShapeKey::new` 1.65%, which is one unavoidable pass over the text — removing *that* is R5, not R2.

## R3 + R4 ☑ Size every per-frame container from the previous frame (2026-08-20)
The single largest win of the series, and the cheapest. `rebuild_inner` built four containers from empty on every rebuild — the node arena, the taffy tree, the `meta` map and the `built` vector — so each grew by doubling and memmoved its contents at every step. PROF1 measured that as **7.9%** in taffy's slotmap (`leaf_ref` → `try_insert_with_key` → `RawVec::grow_one` → `realloc`), **5.6%** in the hashbrown inserts and **~1.1%** in the arena. All four are now sized from `self.prev_tree.len()`. *New API, both defaulted or additive:* `Tree::with_capacity`, `LayoutTree::with_capacity`, and a **defaulted** `LayoutEngine::with_capacity` so an engine with no notion of capacity is unaffected. `LayoutTree::abs` also moved to FxHash (R1's argument, same shape of key). *Measured:* **2323 → 1625 µs, 30%** on top of R1+R2 — and the p90 collapsed from 3773 µs to ~1700, because the long tail *was* the reallocation spikes. The no-text floor went 0.516 → 0.437 µs/node. **Cumulative R1–R4: 2467 → 1625 µs, 34%.** The count is a hint, not a contract: a frame that grows past it reallocates once, exactly as before.

## R5 ◐ Shaped text per node — the achievable half only (2026-08-20)
**R5 as PROF1 specified it — store the shaped block on the node, iced's model — presupposes R6 and cannot land before it.** iced can keep a `Paragraph` in `tree.state` because its widget tree *persists*; Lumen's `Element` tree is rebuilt every frame, so there is no per-node home for the block to live in. A content-keyed global cache is not a design mistake there, it is the only option available without retained nodes. Recorded as blocked-on-R6 rather than attempted. *What did land:* `shaped_run` built a `ShapeKey` for its own `(ShapeKey, scale)` lookup and then called `shaped`, which built a second one — the text hashed **twice per painted node per frame**. The key is now threaded through a `shaped_by_key` core. *Measured:* `ShapeKey::new` 2.20% → 1.97% of a frame, and no frame-time change outside noise — because this benchmark runs a `NullRenderer` and `shaped_run` is only 0.14% of it. The redundancy was real and is gone; the benchmark that would show it is a painting one, which is not what BENCH2's harness is.

## R6 ◐ Retained layout scratch landed; full F2 specified, not landed (2026-08-20)
**What landed.** The layout engine is genuinely per-frame scratch — the solved bounds are copied into the node arena and the tree discarded — but it was *constructed and dropped* every frame. It is now retained on `Headless` and `clear`ed instead, so its capacity survives. `LayoutTree::clear` and a **defaulted** `LayoutEngine::clear` (a fresh engine, for implementations with nothing to retain). *Measured honestly:* `drop_in_place<LayoutTree>` **left the profile entirely** (was 1.58%) and allocator traffic fell from 13.3% to 11.1% of the frame — but the wall-clock delta is inside this box's ±3% noise floor, so it is reported as profile-confirmed rather than as a frame-time win. Kept because it is measurable and removes work; not claimed as a speedup.

**What did not land, and why.** Full F2 — stable `NodeIndex` across frames, so an unchanged subtree needs no work at all — is still the structural fix and is still blocked on what ADR-007 recorded: incremental layout across disjoint taffy subtrees. The profile now *quantifies* the prize rather than asserting it. After R1–R6 the remaining frame is `build_node` 14.5%, taffy ~16% (`compute_preliminary` 6.6, `compute_child_layout` 4.4, `to_taffy` 2.8, rounding 1.5), and memmove 16.4% — and memmove is **no longer growth**: it is now the direct copy of taffy's `Style` into the slotmap (5.5%) and of `LayoutStyle`/`NodeMeta` (3.6%), which only node *reuse* removes. Attempting it here would be a multi-day architectural change to the framework's core with a known blocker, on a branch already carrying six perf commits; specified with evidence is the honest stopping point.

**One thing R1–R6 changed about F2's premise.** Memoization used to buy 10% (2655 → 2377 µs). It now buys **nothing** — 1794 µs plain against 1820 µs memoized. R3/R4 made the plain path cheap enough that the copy path's own bookkeeping cancels its benefit, so "make memoization automatic" (PROF1's R4) is now *worse* than neutral until F2 lands. That inverts the ordering PROF1 recommended and is the single most useful thing this series learned about the roadmap. **Superseded by F2.1 the next day — see below: memoization is worth ~17% again.**

## F2.1 ☑ Memo-hit spans reuse their taffy nodes (2026-08-20)
`docs/plan-f2-splice.md`. R6 named node *reuse* as the only thing that removes the remaining memmove, and this is the first stage of it: a span that hits its memo keeps the taffy nodes it was laid out with instead of re-minting them. The layout tree is no longer cleared each frame; `copy_node` hands back `prev_tree.lnode(prev)`, and the nodes that were *not* reused are freed afterwards.

**Measured before building it** (`benches-competitive/src/bin/probe_f2_reparent.rs`, taskset, 3000 rows): re-minting the tree costs **540.8 µs** against **300.1 µs** for reuse, with a **85.7 µs** floor when nothing is dirty. The probe also settled the design question — adopting reused children into the new parent *first* and removing the stale parent *second* leaves layout correct with a flat node count, so no detach step is needed.

**The probe still gave a false negative, and that is the lesson.** taffy's `remove` nulls the parent pointer of every node in the removed node's child list; the probe checked `compute_layout` output and `child_count`, both of which read the *children* list walking downward, so the corruption of the `parents` array was invisible to it. In the runtime it surfaced as an accumulating fault: a stale container nulls a reused span root's parent, and frames later — when that span is finally re-lowered — taffy's `children.retain` cleans the wrong list and leaves a dead key in a live container, which panics with *"invalid SlotMap key used"* (`virtual_list_memo.rs` caught it). **Fixed by removal order:** stale nodes are freed parent-before-child in `prev_tree` preorder, which is sound because `copy_span` copies whole subtrees, so a stale node's ancestors are always stale too. The walk skips reused subtrees, keeping it O(stale) rather than O(tree).

**Guarded on the previous frame's copy count.** Retaining costs a per-node `remove` for everything not reused, where `clear` releases the tree in one call; on a build that copies nothing that is pure loss, measured at **+7.9%** on the no-scope `build_frame/lumen` before the guard. `layout_reuse` therefore requires `prev_nodes_copied > 0` — a prediction that can be wrong for exactly one frame and is never wrong about correctness. *(Recorded because it was got wrong once: the count was first captured at the top of `rebuild_inner`, but `pump` zeroes it before that runs, so the predicate always read 0 and silently disabled the whole optimisation while every test still passed.)*

*New API, all additive or defaulted:* `Tree::{lnode, set_lnode}` (a dense `u64` slot per node — `LayoutNode::{raw, from_raw}` exist so `lumen-core` need not name `lumen-layout`'s type); `LayoutTree::{remove, node_count}`; and **defaulted** `LayoutEngine::{retains_nodes, remove, node_count}`, so `engine_seam.rs`'s `StackEngine` and any engine with no cross-frame node identity keep today's clear-and-rebuild behaviour untouched.

*Measured:* `build_frame/lumen_memoized/3000_rows` **1812 → 1483 µs, −18.3%**. The full-rebuild path is unchanged within noise (1.77 → 1.79 ms, overlapping intervals; the residual is the new per-node slot). A `debug_assert` compares the engine's live node count against the arena's every frame, so a leak in the reuse path fails the test suite rather than growing quietly.

**What is left.** B − C in the probe — about **215 µs** — is the adoption itself: `vs_iced.rs` scopes each row but leaves `widgets::column(rows)` *outside* the memo, so the container is re-minted every frame and rewrites 3000 parent pointers. Reaching the 85.7 µs floor needs an unchanged container to keep its own node, which is F2.2/F2.3, not this stage. Coverage for the path came first: `crates/lumen-widgets/tests/copy_forward.rs` had been an empty 1-byte file since July while three places in `docs/plan-retained-pipeline.md` cited it as the guard.

## F2.2 + F2.3 ☑ Splice-in-place — the arena is retained (2026-08-20)
`docs/plan-f2-splice.md`. F2.1 reused a memo-hit span's *taffy* nodes; F2.2 retains the **node arena** itself, so a spliced span keeps its `NodeIndex` — and with identity retained, F2.3 ("stop walking copied spans") falls out rather than being a separate stage. `copy_span`/`copy_node` are gone. `splice_span` detaches the span *root*, re-attaches it under its new parent, and never descends. Everything the copy path did per node is now a non-event: side-table entries never move (same key), interaction flags never need refreshing (the node never left the tree, and `restyle_visual` keeps them current), and nested span records need no remapping (they still name the same roots — they are carried forward by testing whether their root survived the free walk).

*Measured:* `build_frame/lumen_memoized/3000_rows` **1483 → 946 µs, −35.7%**; cumulative across F2.1+F2.2 **1812 → 946 µs, −47.8%**. The full-rebuild path is unchanged within noise. Against iced on the same bench: **970 µs vs 290 µs, a 3.3× gap**, from 8.3× when PROF1 started.

**A.3.3's acceptance, precisely.** The *build* is now O(changed): a memo hit touches one node. The bounds/clip pass is O(live) and stays that way on purpose — when anything resizes, the absolute position of every node below it genuinely changes, so there is no smaller correct set. Removing the clip computation entirely was measured at **0.85%**, so the pass is not worth the correctness risk of a staleness heuristic.

**Three things this got wrong on the way, all caught by guards rather than by review:**
1. *The layout tree and the arena must reset together.* The F2.1 `layout.clear()` fast path wiped taffy nodes that the now-retained arena still pointed at; taffy reports that as a panic inside `new_with_children` the moment a rebuilt container adopts one.
2. *The reuse predictor deadlocked.* Gating on "did the last build splice anything" disables the retained arena that splicing requires, so the count can never rise again. It is gated on "does the previous build have any spans" — known before the build.
3. *The container re-pass read bounds before `compute` ran*, because reordering the free pass moved it ahead of layout.

**The coherence oracle needed one change, and it is a real narrowing.** It compared `SemanticsNode::index` — the raw arena slot — which now legitimately differs between a spliced view and a from-scratch rebuild. That field's own documentation already called it "NOT an identity … will be recycled once the arena persists"; F2.2 is that case arriving. `assert_view_coherent` masks the slot and still compares `SemanticsNode::node`, the path-derived handle that exists precisely to survive slot recycling — so a span spliced into the wrong place is still caught. `.ai_docs/03-spec-semantics-agent.md` and the field's own doc now say the slot is stable for unchanged subtrees and recycled for changed ones, the opposite of the pre-F2.2 behaviour.

*New API:* `Tree::{insert_orphan, set_root, detach, attach_last_child, free_one, iter_live, subtree_preorder}`, plus a `prev_sibling` array making the child list doubly linked so `detach` is O(1) — splice detaches a span root every frame, and a singly-linked list degrades to O(n²) on a list whose changed and unchanged rows alternate. `check_invariants` verifies the new back-pointers (ablating their maintenance fails 6 core tests). *Deleted:* `prev_tree`, `prev_meta`, `prev_node_style`, `prev_node_computed`, `prev_layout_style`, and CP1's `prev_spans_by_root` index — none of that work exists once the arena is retained. `build_node` also lost its `built` accumulator; the taffy handle is recorded on the arena node at creation instead.

**A latent F2.1 hazard, found and pinned rather than assumed away.** taffy's `new_with_children` sets the child's parent pointer but leaves it in its *old* parent's child list, so freeing the old parent nulls a pointer a live container has just claimed — reproduced directly at the taffy level, ending in "invalid SlotMap key used". The runtime turns out to be structurally immune: a span's nodes are only freed when its scope re-runs, which requires its enclosing scope to have re-run, which rebuilds the enclosing container in the same frame — so the container holding a freed span root is always dying too, and the parent-before-child free order drops its child list first. `tests/copy_forward_nested_churn.rs` drives the alternation that would expose a regression in that invariant.

*Next, and now the largest single item:* `slots: HashMap<SignalId, Slot>` in `lumen-core/src/state.rs` is a std `HashMap`, i.e. SipHash, and `ReadSet::is_current` probes it once per dep per scope per frame — **8.4%** of the memoized frame (`sip::Hasher::write` 4.79 + `hash_one` 3.65). Same shape of finding, and same fix, as R1's shape-cache hasher.

## F2.4 ☑ The build's own hashers — SipHash out of the hot path (2026-08-21)
Follow-up to F2.2, and a **worked example of getting attribution wrong twice before getting it right**. The post-F2.2 profile showed `sip::Hasher::write` 4.71% + `hash_one` 3.61% = **8.3%** of a memoized 3000-row frame. The first guess — `slots: HashMap<SignalId, Slot>` in `state.rs`, probed by `ReadSet::is_current` once per dep per scope — was wrong: switching it to FxHash left the profile unchanged. Only a caller trace (`perf -g caller`) found the real sources, both inside `rebuild_inner`:

* **`span_ctx_hash`** built a `DefaultHasher` **once per scope per build** and hashed the whole ancestor descriptor stack — all string traffic, 3000 times a frame on the one-scope-per-row shape the F-series recommends.
* **`scope_live` / `scope_skipped`**, two `std::collections::HashSet<IdHash>` written once per scope per build — 6000 SipHash-of-`u128` inserts a frame.

*Fixed:* `span_ctx_hash` and the style-memo key now use `IdHasher` (ADR-021's own construction) and `finish128`, so `SpanRec::ctx_hash` and `style_memo`'s key are **128-bit, not 64**. That is not tidiness: an equal `ctx_hash` makes the runtime *splice* a span instead of re-lowering it, so a collision is a wrong view rather than a slow frame — the same trade R2 made for `ShapeKey`. The two liveness sets moved to FxHash; they are only ever `insert`/`contains`/`clear`ed, never iterated, so no output order depends on them.

*Measured:* `build_frame/lumen_memoized/3000_rows` **942 → 896 µs, −4.9%** (A/B against the same commit, machine idle). SipHash is now **absent from the profile** below a 0.3% threshold. Full-rebuild path unchanged.

*Kept but explicitly NOT claimed as a speedup:* the `state.rs` FxHash swap measured **899 vs 896 µs — inside noise** when isolated. It stays for two reasons that are not performance: the keys are dense `u32` ids the runtime mints itself, so SipHash's DoS resistance buys nothing; and std's `RandomState` reseeds per process, which made `adopt_pending_live`'s restore-diagnostic order vary run to run — FxHash is seed-free, so that order is now stable. Checked against the CP3.1 trap `fxhash`'s module doc warns about: `snapshot()` writes into a `serde_json::Map`, which is a `BTreeMap` here (serde_json is built without `preserve_order` — no `indexmap` in the lock), so its JSON is sorted by key and hasher-independent.

**Method note worth keeping.** Two measurements during this work were nonsense because a user process (`accounts_slice-`, 121–155% CPU) was loading the box — one of them read as a **+38% regression** with a 1.14–1.35 ms interval. The tell was the interval width, not the mean: every honest measurement on this box lands inside ±0.5%. Check `ps` before believing a bench, and A/B against the same commit under the same load rather than against a number recorded earlier.

## F3.5 ☑ Text bindings patch instead of rebuilding (2026-08-22)
`BoundBg` was the only retained binding class, and `NodeDeps` recorded the rule: "background deps update via a paint-only patch; scope/text via a rebuild". Text was structural because a new string can measure to a new size — **true of some values, not of the binding**. `probe_tiers` priced the two paths on the same 3000-row list and the gap was the whole argument:

| tier | cost |
|---|---:|
| idle pump | 0.1 µs |
| background binding (patch) | 59.1 µs |
| **text binding, same size (patch, F3.5)** | **93.2 µs** |
| text change through the scope memo (rebuild) | 832.3 µs |

**8.9×**, and it puts Lumen under iced's 290 µs on this shape.

**How it decides.** `BoundText` retains what the build measured — the wrap width, the ceiled block size, and crucially `auto_w`/`auto_h`: whether the measurement actually *fed* the layout style on that axis. On a change the runtime re-shapes and compares only the axes the measurement owns. Same size ⇒ the node's `LayoutStyle` would come out identical ⇒ no relayout is possible ⇒ patch. Different ⇒ rebuild, which is always correct. The `auto_*` distinction is what makes the fast path fire in practice rather than almost never: an author-fixed width, a `VirtualList` item height, or a paragraph that still wraps to the same lines can never move, so those patch even when the glyphs get wider.

**Two-phase commit.** Every stale binding is evaluated and measured before anything is written, so "one binding would move layout" declines the whole pump cleanly instead of leaving some nodes patched and others stale. `one_layout_moving_binding_forces_the_whole_pump_to_rebuild` pins that.

**Why isolating the reads cannot strand a memoized subtree.** `dyn_text` now uses `eval_isolated`, so its reads no longer enter `structural_reads` — which is exactly what stops them forcing a rebuild. The staleness that would otherwise imply is already prevented upstream: `build_node` increments `impure_seen` for any node carrying a `dyn_text`, and `splice_span` refuses to splice an impure span. So a rebuild always re-lowers the node and re-evaluates the binding. A `debug_assert`-backed fallback re-marks the reads structural if a binding is ever evaluated on a node that never reaches the text sizing block.

**The patch invalidates semantics; the background patch does not and should not.** Text is the node's accessible label as well as its content, so `patch_text_bindings` clears `sem_root` and the elided/JSON caches, and writes `m.label` alongside `m.content` — a patch that updated only one would drift from what a rebuild produces, which is exactly what `assert_view_coherent` compares. Ablating the label write fails 2 of the 4 tests.

*Verified by ablation* (`tests/bound_text_patch.rs`): making the patch always decline fails 2/4; removing the size check fails 2/4; dropping the label write fails 2/4.

*Not covered:* a node that ellipsizes. The painted string is then a derived truncation rather than the binding's value, and reproducing it in the patch path would mean duplicating that logic — `patchable: false`, always rebuilds.

## F3.6 ☑ Bindings no longer bar a span from the splice path (2026-08-22)
Step 1 of three, and a **prerequisite, not an optimisation**: `build_node` marked `impure_seen` for any node carrying a `dyn_text` or `dyn_bg`, and `splice_span` refuses an impure span. So a single bound label anywhere in a list made that whole span re-lower on every rebuild — which is exactly why "make all text a binding" (step 2) would have been a large regression rather than a win.

Measured on 3000 rows where **every** row carries a bound label, forced through the rebuild path by a structural change:

| | rebuild cost |
|---|---:|
| old `impure` rule — nothing splices | 3065.1 µs |
| F3.6 — bound spans splice | **869.2 µs** |

**3.5×.** Note the old number is worse than a plain non-memoized rebuild (~1741 µs): the bindings are evaluated *on top of* re-lowering everything. A one-bound-row probe shows none of this — the rule only bit spans that carried a binding — which is why the first measurement of this was thrown away and redone.

**Why the ban existed, and why it no longer needs to.** A spliced span reuses last frame's `meta`, so a binding whose signal moved since then would come back stale. `settle_bindings_for_rebuild` removes the premise: before a rebuild chooses what to splice, every stale binding is brought up to date in `meta` — backgrounds always (paint-only), text when the new string measures the same size (the F3.5 test). When a text refresh *would* move layout, the view caches are dropped so nothing splices and `build_node` re-evaluates everything against fresh layout. Coarse — only the enclosing scopes strictly need invalidating — but it is the rare branch (a size-changing text update landing in the same pump as a structural change) and being coarse there can only be slow, never wrong.

`dyn_classes` stays impure: classes drive the `.lss` cascade, so a change can resize anything in the subtree and there is no cheap "would this cascade differently" check. `Custom`/`Canvas` stay impure because their output is an arbitrary closure.

**Binding records are now carried across a splice**, beside the F2.2 span carry-forward and by the same test: a re-lowered node was allocated a fresh index and its old one freed, so "still alive" is exactly "spliced".

*Ablations* (`tests/bound_text_patch.rs`, 8 tests): removing the settle pass fails 2; removing its size check fails 1; **dropping the binding carry-forward failed nothing** — the suite changed a binding only *before* its span was ever spliced, so losing the record showed up as a label that silently stops updating, with no failing frame at the time. `a_binding_still_updates_after_its_span_has_been_spliced` drives splice-then-change and now fails that ablation. Worth recording as the shape of gap that ablation catches and review does not.

## F3.7 ☑ `impl Into<Text>` — the reactive form is now the default one (2026-08-22)
Steps 2 and 3 of three (step 1 was F3.6). `Prop<T>` had sat in `lumen-core/src/binding.rs` since F3 with **zero call sites** — built for exactly this and never adopted. Every widget that renders author-supplied text now takes `impl Into<Text>`, so a binding is passed straight in rather than reached through `.bind_text(..)` on a bare text element:

```rust
widgets::text(bind!(rt => format!("{} items", count.get(rt))))
widgets::button(bind!(rt => label.get(rt)), on_click)
widgets::radio(cx, "grp", 0, bind!(rt => name.get(rt)))
```

**Why the conversions are spelled out rather than blanket.** `impl<T: Into<String>> From<T> for Text` cannot coexist with `From<Dynamic<String>>` — Rust's coherence rules forbid negative reasoning, so the compiler cannot be told `Dynamic<String>` will never implement `Into<String>`. Taking the blanket would leave bindings needing a separate entry point, which is the exact ergonomic problem being solved. So `&str`, `String`, `&String`, `Cow<str>`, `Dynamic<String>` and `Prop<String>` are listed individually.

**`Text::map` is what keeps composing widgets on the fast path.** A radio renders `◉ {label}`, a switch its state glyph — without a way to compose *through* a binding those widgets would have to force the value to a `String`, silently dropping back to the rebuild path. `map` wraps a `Dynamic` in a new one and passes a `Static` straight through.

**Blast radius, measured rather than guessed:** 20 constructors in `lumen-widgets` and **25 helper functions across 7 example crates**, each a one-line widening of `impl Into<String>` → `impl Into<Text>`. Source-compatible for direct call sites (`&str`/`String` still convert); a *generic* helper of the author's own must widen its own bound. **Downstream consumers (Mercurium) will need the same one-line change on any `fn helper(s: impl Into<String>)` that forwards to a Lumen text widget.**

Two constructors deliberately keep the string on the parent and put the binding on the child that actually renders it — `chrono-stopwatch`'s pill button and `Card::title` — because the parent uses the label as its accessible name, which is not the thing being painted.

*Ablations* (`tests/bound_text_patch.rs`, 10 tests): making `into_parts` drop the binding fails 2; making `Text::map` drop it fails exactly the composed-label test.

**Step 3 — the guidance.** `.claude/skills/building-apps` and the `text!` re-export doc now say plainly that reading a signal in the view body and interpolating its *value* is the slow form (structural read ⇒ rebuild), and that `bind!`/`text!` is the default. The API change is only worth its blast radius if authors actually write the binding form, which is a documentation problem, not a compiler one.

## LN3 ☑ The ICU dictionary becomes opt-out — shipped profile −3.6 MB (2026-08-23)
`docs/binary-size-2026-08-22.md`. BENCH4 left "Lumen's executable is ~50% above iced's" as an observation; the investigation found **69% of the 5.3 MB gap was a single 3.62 MB blob** — ICU4X's `cjdict`, reached through parley's `complex-scripts`, which the workspace manifest had hardcoded.

**The manifest's justification was wrong on both counts.** It read: *"without it parley panics (\"no segmentation model for language: ja\") on CJK"*. Measured (`lumen-text/examples/cjk_probe.rs`, wrapping at 160 px): it does not panic — ICU records a data error and the segmenter falls back — and **ja/zh wrap identically with and without it**, because CJK has break opportunities between most characters. What the dictionary actually buys is **Thai** (222.8 px unwrapped → 127.6 px wrapped), and by the same mechanism Lao/Khmer/Burmese, plus word-granularity cursor movement and double-click selection in CJK.

*Landed:* `complex-scripts` on `lumen-text`, forwarded through `lumen-app`, `lumen-widgets`, `lumen-shell`, `lumen-agent`, the facade and all four platform shells — the same chain `pan-unicode` already used — and added to every one of their `default` sets, so a full build is unchanged.

| build | before | after |
|---|---:|---:|
| `hello` (default) | 7.7 MB | 7.7 MB |
| `hello` (pan-unicode) | 22.2 MB | 22.2 MB |
| lean-app | 6.9 MB | **3.3 MB** |
| win-app — what a user ships | 14.0 MB | **10.4 MB** |
| nogpu-app | 10.9 MB | **7.3 MB** |

**The lean profiles were not opted out by hand.** They already pass `default-features = false`, so converting a hardcoded parley feature into a default dropped it from all three at once — the scaffolded lean app halved, and ADR-CFG1's **<5 MB target is met for the first time (3.3 MB)**. Coherent rather than a regression: those profiles embed only the Latin+symbols face and could not draw a Thai glyph with or without the segmenter. An app that registers a wider face at runtime turns the feature back on.

Size-gate ceilings re-tightened so the saving cannot be given back silently: lean 8 → 5 MB, windowed 16 → 12, no-GPU 13 → 9.

*The stderr concern raised in the investigation was wrong and needed no fix.* `icu_provider` aliases its `warn` to `eprintln` only under `debug_assertions`; a release binary with the feature off is silent, verified by running one.

*Guarded by* `crates/lumen-text/tests/complex_scripts.rs`, which asserts **opposite outcomes per feature state** — Thai must wrap with it and must overflow without it — so neither reinstating the hardcoded dependency nor quietly dropping the feature from the defaults can pass. Ablating the forwarding fails it.

## O ☑ Agent observability — what the agent cannot see of a running app (2026-08-24)
`docs/plan-agent-observability.md` (rev 2, after three expert reviews); evidence in `docs/review-agent-observability-2026-08.md`.

**COMPLETE 2026-08-24** — all phases O0–O5 landed across 26 commits, every one with `just ci` green and docs updated in the same commit. Nine new diagnostics (W0111–W0117, W0303, W0403), six new/extended agent methods, and the ambient audit that turns the whole lint surface from pull into push at **+2.4%** frame cost.

*Two defects were found by verification rather than by review, and both are recorded above where they were fixed:* `ui.lastDamage` returning `none` for a click that demonstrably repainted (live-window only — headless structurally could not reproduce it), and the occlusion check silently never firing because `NodeMeta.background` holds only the *typed* element background, not a `.lss`-set one.

**Premise.** Lumen's agent surface is strong but *interrogative* — `ui.explain`, `ui.getStyles`, `ui.probe` answer well only if you already suspect a node and can name it. Human sight is ambient, push, and hypothesis-free. The phase adds a per-frame dev-build audit that volunteers what a human notices, written into the existing `Runtime::log` ring.

- [x] **O1.1 ☑ Contrast reaches `ui.lint` (W0303).** `analyze_contrast` + `contrast_report()` were fully implemented, tested, and had **no caller on the lint path** — so white-on-white text was undetectable while `03 §ui.lint` had claimed contrast coverage since before it existed. A **legibility** floor (|Lc| < 15), not a design opinion: `ContrastLevel::Fail` starts at 45 and would fire on legitimate secondary text. *Guarded by* `crates/lumen-widgets/tests/contrast_lint.rs`, which pins all three regimes — invisible fires, readable does not, and mid-grey-on-white (poor design, legitimate output) does not.
- [x] **O1.2 ☑ `ui.lastDamage`.** Damage has been computed every frame since R2 (it drives the shell idle-skip and the GPU scissor) and was reachable from Rust and from `lumen-test`'s tracer, but the string `damage` did not appear in `lumen-agent` at all. Returns the rect **and** the nodes intersecting it — rects alone force a spatial join the agent has no primitive for. `nodes` is populated for `region` only: under `full`, "which nodes" is every node and therefore no information. Also re-exports `Damage`, which was the type of the public `FrameStats::damage` field and of `last_damage()` without being nameable outside the crate. *Guarded by* `crates/lumen-agent/tests/last_damage.rs`, which asserts a one-node text change stays a **bounded region** naming exactly that node — accepting `full` there would let a damage-precision regression pass silently.
- [x] **O1.3 ☑ `app.perf` completed + session facts.** `style_memo_stats()` had **zero callers**; `nodes_rebuilt`/`nodes_copied` were maintained every frame and absent from the protocol. The blocker review found: both are zeroed at the top of *every* pump, and `ui.waitSettled` loops until quiescent — so the recommended interact → settle → measure sequence read 0/0 regardless of what happened. Fixed by adding **cumulative** totals beside the per-frame ones (`FrameStats` semantics unchanged, so existing tests still mean what they meant). Added `frame_ms_max` (all-time, since rolling percentiles forget a stall within ~2 s) and `frames_over_budget`/`frame_budget_ms`. Session facts (`renderer`, `is_gpu`) are **queryable**, not merely logged — `take_diagnostics()` clears on read, so a startup-only warning is unrecoverable for an agent that attaches later. New `Renderer::is_gpu` (default `false`) makes the silent `WgpuFallbackTinySkia` CPU degradation observable; new `TextEngineApi::cache_stats` exposes the thrash indicator. *Guarded by* `crates/lumen-agent/tests/perf_counters.rs`, whose first test runs the exact interact → `ui.waitSettled` → `app.perf` sequence that defeated the per-frame counters.
- [x] **O0.3a ☑ `lint()` stops re-shaping the world — 1.20 ms → 0.34 ms** (200-label screen, release, warm cache; 20-call mean). The tofu loop called `TextEngineApi::layout`, which **bypasses the cache entirely** — it is the uncached primitive `shaped_by_key` calls on a miss — so every `lint()` re-shaped every text node from scratch, under a comment asserting *"Shaping hits the cache, so this is a cheap walk."* Swapped to `shaped`, whose `ShapeKey` hashes exactly `(text, style, wrap, align)`, so build/paint have already populated the entry. Separately, `lint()`/`diagnostics()` sourced their root from `semantics_doc()`, which deep-clones the whole tree (a `String`/`Vec` per node) to hand out a reference; they now borrow the memoized `sem_root()` `Rc`. **Both fixes benefit existing pull-mode `ui.lint` callers**, independent of the ambient audit, and likely make O0.3's frame-cadence throttle unnecessary. `tofu_lint_flags_uncovered_glyphs` passes unchanged — the cached path detects `.notdef` identically.
- [x] **O0.1b ☑ Diagnostics carry a machine-readable node anchor.** `.with_node()` had **zero call sites tree-wide** — every check embedded the offending node as free text in `message`, so a consumer had to guess at an unenforced per-check formatting convention, and a `(code, node)` dedup key collapsed every finding of a code onto one slot. Added `Diagnostic.handle` **beside** `node` rather than filling `node` harder: `node` is the author's `#id` and is absent on unnamed nodes — including, by definition, every `W0301`, which is the case that proves the two must be separate fields. Retrofitted W0103/W0104/W0105/W0106/W0108/W0110/W0301/W0402/W0303; `W0001` stays unanchored because it concerns N nodes sharing an id and that id *is* the selector. `Display` now renders the anchor, so the rendered string is self-sufficient. `handle` is `Box<str>`, not `String`: written once, never mutated, and the 8 bytes keep `Diagnostic` at 120 under the 128-byte line where `clippy::result_large_err` fires on two public `Result<_, Diagnostic>` signatures. *Guarded by* `crates/lumen-widgets/tests/diagnostic_anchors.rs`, whose main test asserts two same-code findings carry **distinct** handles — the exact collapse that would have silently defeated the ambient audit.
- [x] **O0.1 ☑ `dev-observability` feature**, default-on, forwarded core → app → widgets → shell/agent/facade. **Narrowed in rev 2 to gate only O0.3 and O2.3**: everything else in this phase is a latched boolean or a one-shot line, the same cost as the always-on `atlas_overflow` latch, and ships ungated. Deliberately not tied to `snapshot` (these checks need no `serde_json`); the original justification — lean-build-with-agent — was falsified and is recorded as such. Two crates pull `lumen-widgets` with `default-features = false` (`lumen-agent`, `lumen-shell`), so a *default* feature does not survive: both forward it explicitly. Missing that is what made the first O0.3 test run report nothing.
- [x] **O0.2 ☑ edge-triggering primitives** (`lumen-core::observe`): `Latch`, `FrameDiff`, `Throttle`. `FrameDiff` is a per-pass **presence diff**, not a monotonic seen-set — the latter needs an invalidation policy and every candidate is wrong (clearing on `rebuild_fresh()` misses ordinary state-driven rebuilds, since `pump` calls `rebuild()`).
- [x] **O0.3 ☑ the ambient audit — lint goes from pull to push.** Each qualifying pump diffs `lint()` findings and logs the newly-appeared ones to the ring, so **every lint that exists today and every one added later is push-mode for free**, with no protocol change. Keyed on `(code, handle)` — messages carry measured values that jitter during animation and would defeat dedup. **`stats.painted` alone was not a sufficient gate**: `set_stylesheet`/`set_theme`/`resize` rebuild *outside* `pump`, so the pump after a hot stylesheet edit is idle — exactly the moment a developer wants to hear about a new finding. Gated on painted-or-tree-generation-changed instead. **Cost: 11.09 → 11.35 ms/frame (+2.4%)** on a 200-node rebuild-heavy frame — inside the 2× bar, and only because O0.3a landed first; at the pre-O0.3a `lint()` cost this would have been ~11%. *Guarded by* `crates/lumen-agent/tests/ambient_audit.rs`: one entry per held finding across 25 frames, two nodes of one code as two entries, a fixed-then-re-broken finding reported **again**, and a clean app logging nothing.
- [x] **O0.4 ☑ The ambient audit was O(n²) — 42.2 ms → 6.1 ms** on a 6602-node animated frame. `handle_for_index` walked the whole semantics tree *per call*, and the audit calls it once per finding candidate; a long page therefore paid a full tree walk thousands of times a frame. Memoized into a `handle_index_map`, invalidated with the rest of the semantics cache. **This one bug had been distorting every proportion measured off an animated frame** — including the "columnar `NodeMeta` is worth ≤5%" bound recorded in `docs/report-direct-lowering-2026-08.md`, which was computed against a frame that was 99% this walk.
- [x] **O0.5 ☑ Findings are capped per code** (`MAX_PER_CODE = 50`, plus a `suppressed_note()` tail). A long page reported **6372 findings a frame**, all formatted into `String`s and immediately deduped away. The loops keep *counting* past the cap and stop *formatting*, so the suppressed tail is reported as a number rather than silently dropped. Applies to `offscreen_findings`, `invisible_findings`, `truncation_findings` and the tofu scan. Note this is a **default-on** cost: `dev-observability` is in `default`, so an ordinary `cargo build --release` runs the audit; only `--no-default-features` drops it.
- [x] **O0.14 ☑ `Element` splits hot from rare — 1072 → 784 bytes.** The last `build_node` phase-table row (`view`). The same fourteen fields O0.13 moved out of `NodeMeta` — every event handler past `on_click`, caret/selection, scroll state, shadow — were **304 of `Element`'s 1072 bytes**, and a view function materializes the *whole* element tree at once, so they were written as `None` per node per frame for nothing. `type_sizes.rs` had already named this fix in a comment ("the answer is EL, bundle the rarely-set fields behind one pointer") and its `assert_size!` is what caught the change. Frame **2297 → 2215 µs (−3.6%)**, plus 288 B/node less transient memory. The migration was cheap for a reason worth recording: **outside `lumen-app` almost nothing assigned these fields directly** — a field-by-field count found zero external writes for thirteen of the fourteen, and `shadow` (29 sites, all examples) already had a builder. Struct literals needed `#[doc(hidden)] set_*(Option<T>)` setters because `..Default::default()` requires every field to be nameable, which is also why `rare` is `pub`. *Guarded by* `type_sizes.rs::element_size_is_pinned`, updated with the new constant and the reasoning, per its own instruction.
- [x] **O0.15 ☑ The ambient audit is throttled — −28.6% on a changed frame.** It is a *push* channel for a human or an agent, so its contract is "tell me promptly", not "tell me this exact frame" — but a 60 fps animation asked it sixty times a second, at 858 µs a pass on a 4000-node page (27% of the frame, measured with the audit compiled out as the control). Now gated on `stale && (settled || due)`, 100 ms interval. `stale` is unchanged from O0.3 and is still the primary gate: nothing to say unless the tree moved since the last audit, so a static app never audits regardless. **Getting `settled` right took three attempts, and the value of the exercise was in how the wrong two failed.** `!stats.painted` fires on *every* frame of a rebuild whose output is pixel-identical — exactly the workload being throttled — so it changed nothing at all (audit runs 51/51); "same generation as the previous pump" then missed rebuilds that happen *outside* `pump` (`set_stylesheet`, `set_theme`, `resize`), which broke `a_reintroduced_finding_is_reported_again` — the very case O0.3 was written for. The definition that holds is "this pump did not itself move the tree", which covers the animation-stops case and the out-of-band-rebuild case together. Audit runs **51 → 1** on the bench; frame **3218 → 2297 µs**. Accepted cost: a finding that appears *and* disappears inside one 100 ms window during continuous animation is not seen. `ui.lint` is unaffected and still answers exactly, immediately, on demand. *Guarded by* the existing six ambient-audit tests plus `a_finding_introduced_mid_animation_is_reported_when_the_tree_settles`.
- [x] **O0.13 ☑ `NodeMeta` splits hot from rare — 816 → 528 bytes.** The last `build_node` phase-table item. Every node carried all thirteen event-handler `Option`s past `on_click`, plus caret/selection, scroll state and shadow — **304 of 816 bytes present as `None` on every label in every list**. `meta` is not only written once per node per rebuild but *walked* several times a frame by the audit passes, so those bytes were paid on both. Moved behind `Option<Box<RareMeta>>`, allocated only when a node actually has one of them. Field access became fourteen accessor methods; deleting the fields made the compiler enumerate all 61 call sites exhaustively, which is why this was a mechanical change rather than a risky one. Result: **−35% on `NodeMeta`, −288 bytes/node retained** (1.15 MB at 4000 nodes), frame **median −1.8%, min unchanged** — reported as measured. The memory number is the real one here; the time saving is small because the insert was already near memory bandwidth (147 µs to move 3.3 MB) and shrinking it moves proportionally less.
- [x] **O0.12 ☑ The tofu audit stops asking, and is told instead — −33% on a changed frame.** Whole-frame instrumentation (not just `build_node`) found `ambient_audit` at **2351 µs of a 4898 µs frame — 49%** — and **1611 µs of that in the T.4 tofu scan alone, 32% of the whole frame**. Two causes, and the obvious one was the smaller: the scan cloned every text node's string, `TextStyle` and id into a staging `Vec` purely to release the `&self.meta` borrow before `&mut self.text` (they are disjoint *fields*, so destructuring removes the vector) — worth only 109 µs. The real cost was **4000 shape-cache lookups at ~286 ns each**: the cache genuinely hits (misses equal the number of distinct strings, shaped once ever — verified with a counter), but its values are large `TextBlock`s so every lookup is memory-bound. The framework was spending a third of each frame confirming that nothing was wrong. Whether anything *is* wrong is decided once, where a run is actually shaped, so `TextEngine` now latches `tofu_seen` on the shaping-miss path and the audit skips the entire walk while the answer is no: tofu **1467 → 82 µs**, frame **4806 → 3220 µs (−33%)**. The trait default is `true`, so an engine that cannot tell gets the full walk exactly as before. *Guarded by* `lint.rs::tofu_appearing_after_a_clean_frame_is_still_reported` — the hazard a latch creates is tofu that arrives *later*, so the test runs six clean frames (proving the fast path is the one being taken), then flips a signal to introduce a private-use codepoint and requires the finding. **Method note:** every earlier item in this series was chosen from a `build_node` phase table, which could only ever see 1/3 of the frame. The single largest cost in the engine was outside it the whole time.
- [x] **O0.11 ☑ The node descriptor joins the memo; states stop allocating.** The interning item. `NodeDesc` cost three allocations per node (the id string, the class vector, the role string) plus one `String` per visual state, for a value that is a pure function of the same identity the A.5b key already collapses. The key is now hashed **from the parts**, length-prefixed per field so `["a","b"]` cannot collide with `["ab"]`, so the descriptor need not exist to look itself up; it rides in the memo entry and a hit returns it as a refcount bump. `states` became `Vec<&str>` — every entry is a literal or already owned by the element, and only the miss path needs owned data. Measured: `cas:desc` 161 → 149 µs; frame **−1.0%** flat/shared-identity (4856 → 4806 @4000), **−2.2%** with a unique id per row (2644 → 2586 @2000), **−3.3%** deep *and* unique-id (2276 → 2202, depth 8). The smallest of the O0.x wins, and reported as such — it is worth roughly a tenth of O0.10. Its worst case (a miss must now materialize the ancestor chain, held as `Rc`s, into the plain slice `resolve_with_ancestors` wants) was measured directly rather than argued away, and is still ahead. `IDS=1` added to `buildphase` to reach that shape.
- [x] **O0.10 ☑ The resolved `Style` is shared too — −12.8% on a changed frame.** The other half of O0.6's pair. `lumen_style::Style` is **1008 bytes**; it was cloned out of the A.5b memo for every node *and* moved into `node_style` — ~4 MB of memcpy each way on a 4000-node frame — for a value byte-identical across every node that resolves alike. Now `Rc<Style>` on both the memo and `node_style` (all 13 read sites take `&`; none needed `get_mut`). The three writers fork with `Rc::make_mut`, and crucially the fork is **gated on whether a write can happen at all**: `apply_transitions` already returned immediately without `transitions` (or a theme window) and `apply_keyframes` without an `animation`, so testing those same conditions on the shared value keeps the overwhelmingly common node on a refcount bump. The test is re-read *after* the inline merge, which can introduce either. Measured at 4000 rows, styled, every node rebuilt: `cas:memo` 210 → 74 µs, `cas:apply` 229 → 128 µs, whole `build_node` **2321 → 1958 µs**, whole frame **5568 → 4856 µs (−12.8%)** over 6 runs an arm. Same lesson as O0.6 and O0.9, now three times over: the cost was not an algorithm but a large value copied where it could have been shared.
- [x] **O0.9 ☑ A retained side table with no readers — −4.6% on a changed frame.** `node_layout_style` held the post-css `LayoutStyle` for every node, written on every rebuild (a 256-byte clone plus a hash insert) and swept per freed node. It existed for **A.3.2's copy-forward path**, where a memo-hit span rebuilt its taffy nodes and wanted the derived style back instead of re-deriving it. **F2.2 replaced copy-forward with splice-in-place** — a spliced span *keeps* its taffy nodes, so nothing rebuilds them — and the map's last reader went with it. It was never removed: a workspace-wide search found one write, one clear, one remove, and **zero reads**. Removing it: 4000-row styled changed frame **5839 → 5568 µs (−4.6%)** over 8 runs an arm, distributions barely overlapping; plus 256 B × nodes of retained memory and its hash-map overhead. *Guarded by* the existing splice suite — `copy_forward`, `copy_forward_nested_churn`, `hover_memo`, `scope_spans`, `virtual_list_memo` — which is exactly the path that would notice if a spliced span still needed the derived style. **The general lesson, which the O0.x series keeps repeating: the expensive thing is usually not a slow algorithm but work retained past the design that needed it.** O0.6 was the same shape (a map only `get_styles` reads, deep-cloned per node per frame), and both were invisible until a phase table asked what each part of `build_node` actually cost.
- [x] **O0.8 ☑ The ancestor-context hash is a lazy prefix, not a per-node walk.** `span_ctx_hash` hashed the whole ancestor descriptor stack — every ancestor's id, classes, states and role — **once per node**, making the per-node style key O(depth) of pure string traffic. It is now memoized per depth on `desc_hash_stack` and filled **lazily**, which is the load-bearing detail: a node is pushed onto the desc stack like any other, but nothing ever asks for the prefix *below a leaf*, so a flat list hashes its shared ancestor once instead of 2000 times. An eager version measured **worse than the walk it replaced** (descpush +62 µs against hash −22 µs) because at depth 1 the old walk was already O(1) with a smaller constant. Result, holding node count ~constant and using definite-size wrappers so the layout blowup below does not drown the signal: depth 0 +0.4%, depth 6 +1.2% (both noise), **depth 12 −4.2%, depth 20 −7.7%** — the monotonic curve an O(depth)→O(1) change should produce, and worth nothing on the flat shape. *Guarded by* a `#[cfg(debug_assertions)]` oracle inside `span_ctx_hash` that recomputes the pre-O0.8 walk and asserts equality on **every call**, because a hash that drifts is not a slow frame but a wrong view — the runtime splices on hash equality. All 687 tests run under it.
- [x] **O0.7 ☑ `ui.lint {"all": true}` — the cap belongs to the ambient pass, not the check.** O0.5's per-code cap is correct for the *push* path, which runs on a frame budget; it is wrong for a caller who explicitly asked and is waiting, whose cost is bounded by the one request and who may be hunting the very node the cap hid. `lint()` (capped) and `lint_all()` (uncapped) now both delegate to `lint_capped(cap)`, and the reply carries `capped` so a reader can tell which it got. The cap was never the expensive half — O0.3a made the underlying scans cheap (cached shaping, borrowed semantics root) and the cap only bounds message *formatting*, which is why lifting it for one call is affordable and lifting it per-frame was not. *Guarded by* `lint_caps.rs::lint_all_reports_every_finding` — which also asserts the capped arm's summary total **equals** `lint_all().len()`, so the cap is proven a display bound rather than a count bound — and `lumen-agent/tests/lint_all.rs` for the wire shape, including that a non-boolean `all` does not silently uncap.
- [x] **O0.6 ☑ The computed-value map is shared, not copied — −10.6% on a styled changed frame** (2000 rows, every node rebuilt, small sheet: 2905 → 2599 µs; at 4000 rows 6864 → 6076, −11.5%). `node_computed` is the *observability* half of the cascade — its only reader is `get_styles`, which is `#[cfg(feature = "snapshot")]` — and it was deep-cloned out of the A.5b memo for **every node of every rebuild**, re-allocating a `String` key per declaration per node. Since the memo key is the node's whole style identity, those copies were byte-identical. Now an `Rc` clone; the two writers that genuinely mutate (an inline style, a restyle) fork via `Rc::make_mut`. Measured breakdown of the 448 µs cascade phase it came from: 172 µs this map, 46 µs the `Style` clone, 101 µs `NodeDesc` construction + hashing. *Guarded by* `inline_style.rs::an_inline_style_does_not_leak_through_the_shared_computed_map` — two id-less identically-classed rows share one memo entry (asserted non-vacuously via `style_memo_stats`), one carries an inline style, and the sibling must still report the sheet value. Swapping `make_mut` for `get_mut` panics in all three inline tests, so the fork is load-bearing.
- [x] **O2.1 ☑ Effectively-transparent nodes are reported (W0111).** `SemanticsNode` carries `bounds`, `ink`, `states`, `text_metrics` and **no opacity or colour at all**, so an `opacity: 0` button was invisible on screen, correctly sized in the tree, hit-testable, labelled, and reported as fine by every tool — while `visibility: hidden` is handled properly (flags cleared, node leaves paint *and* semantics). **Effective opacity did not exist as data**: paint emits nested `PushLayer` and lets the backend composite, so the product is emergent and never stored; `node_style.opacity` is only the node's own. Added `effective_opacity` walking ancestors, **resetting at overlay roots** — a sheet anchors to the window, so it must not inherit a dimmed page's alpha, or the one thing the user can see would report itself invisible. Exposed via `ui.getLayout` (beside `ink`/`text_metrics`, the other per-node visual facts deliberately kept out of the tree) rather than added to `SemanticsNode` — which also sidesteps the `additionalProperties: false` schema gate. Only fires for interactive-or-labelled nodes with real area, and exempts a running opacity animation. *Guarded by* `crates/lumen-widgets/tests/invisible_lint.rs`, incl. the 0.5-inside-0.5 → 0.25 product and the overlay-reset case.
- [x] **O2.2a ☑ W0103 tests all four edges.** It tested only `x1`/`y1`, so a child at `x: -400` sat entirely off the **left** of its parent and raised nothing — the direction a human notices *fastest*, because the content is missing rather than merely cut off. The message now names the edge, since "12 px past the edge" on a left-overflowing node sends the author to the wrong side of the box. **The golden churn the plan budgeted for did not materialize**: 118 test binaries green, unchanged.
- [x] **O2.2b ☑ Off-viewport nodes are reported (W0112).** Nothing checked the window at all. W0103 is parent-relative, so a node sitting correctly inside a parent that is itself off-canvas satisfies it by construction, and the root's own escape was never examined. Exempts scroll subtrees — scrolled out of view is what a scroll container is *for*, and reporting it would fire on every long list and get the check ignored; `ScrollInfo` already answers that question. *Guarded by* `crates/lumen-widgets/tests/offscreen_lint.rs`.
- [x] **O2.5 ☑ The GL-gradient defect is reported (W0115) + backend session facts.** `gpu.rs` has documented since 2026-08-08 that on the GL backend `textureSample` of the gradient ramp returns zeros — *"every gradient in the frame renders as nothing, with no validation error"* — and `adapter_is_gl()` existed with only the parity suite calling it. Nothing told the developer. This is the defect class an agent cannot see by any route: no error, a wholly correct semantic tree, and a screenshot that only looks wrong if you already know better. Latched once per renderer, drained through the existing `take_diagnostics` path. **Also added `Renderer::backend` / `backend_has_known_defects` as queryable session facts on `app.perf`** — `take_diagnostics` clears on read, so the one-shot advisory is gone after the first painted frame and a late-attaching agent could never recover it. *Guarded by* `crates/lumen-render/tests/backend_facts.rs`, which pins the invariant (W0115 iff GL, at most once). **Stated limit:** the GL arm cannot be forced on this box — `WGPU_BACKEND=gl` is overridden by the explicit `backends` sweep — so the test proves the quiet path here and the loud one on a GL-only machine.
- [x] **O2.4 ☑ A blank window says so (W0114).** The most common early-development outcome, and one every per-node lint misses **by design**: each individual zero-area node is defensible — `W0105` fires only on *interactive* ones, because a decorative spacer with no size is not a defect — so a screen where everything collapsed passes every per-node check while the user sees nothing, with a fully populated semantic tree that makes `ui.getTree` look healthy. A whole-frame fact no per-node check can express. Deliberately narrow: "no node has any area", not "almost the whole frame is one colour" — the pixel version needs a rendered frame and a sampling policy and would fire on legitimate single-colour designs. Limit stated in the code: it does not catch a frame painting only background-coloured content; for text that is `W0303`'s job. *Guarded by* `crates/lumen-widgets/tests/blank_frame_lint.rs`, incl. one finding for the frame rather than one per collapsed node.
- [x] **O2.3 ☑ Occluded controls are reported (W0113) — phase O2 complete.** Occlusion was checked in exactly one place: `ui.explain` with `kind: "click"`, considering only `overlay: true` nodes and only the single centre point a synthesized click uses — and `ui.explain` only answers about a node you already suspect. An ordinary sibling raised by `z-index`, or a panel that grew over its neighbour, was reported by nothing. Coverage is measured against the covering node's **clipped** box (intersected with every clipping ancestor and the window), so a panel that is itself mostly scrolled away cannot over-claim. Bounded at 4000 nodes and it **logs the skip** — a silent cap reads as "checked everything, found nothing", the one answer this must never give. **Found a real bug while testing:** `NodeMeta.background` holds only the *typed* element background, so a `.lss`-set background was invisible to the check; it now falls back to the resolved cascade style.

  *Retracted 2026-08-24 — the "incidental finding" recorded here was wrong.* Absolute positioning works: `position: absolute; inset-left: -300px` moves the node to x0 = -300, and `inset-left: 250px; inset-top: 40px` lands it at (250, 40). The probe used CSS's `left`, which is **not** a Lumen property — `04 §3` specifies `inset(-…)`, regularizing the four-sided pattern `padding`/`padding-left`, `margin`/`margin-left`, `inset`/`inset-left`, where CSS itself is inconsistent. Nor was it silent: `set_stylesheet` returned `E0102: unknown property \`left\` (app.lss:1:29)` and rejected the sheet **atomically**, and `"stylesheet rejected (1 diagnostics)"` reached the log ring. The bounds did not move because *no stylesheet applied at all*. The probe discarded both signals. **The real, much smaller gap — since fixed:** `did_you_mean` matches on Levenshtein ≤ 2, and the CSS names Lumen spells differently are not near-misses but different names for the same thing, so the metric structurally could not find them — `left` → `inset-left` is distance 6, `background-color` → `background` 6, `box-shadow` → `shadow` 4. Seven such names got **no suggestion at all**. Now matched by an explicit table (`properties::CSS_ALIASES`) consulted before the distance pass; they stay **errors**, not accepted aliases.
- [x] **O3.1 ☑ Truncated text is knowable (W0403).** `text-overflow: ellipsis` paints a truncated string while the tree keeps the full one — deliberate, and defended in `NodeMeta.display_text`'s own doc comment: truncating the tree would make `ui.getTree` report `"Some long lab…"` and corrupt the observability surface to fix a visual one. That reasoning is right and is **not** reversed here. What it left behind was an agent confidently wrong — screen reads `Quarterly rev…`, tree reads the full label, `assertText` passes, and "the column is too narrow" is invisible. Adds the missing third option: keep the label full **and** report the split, via `ui.getLayout.painted_text`/`truncated` and W0403. Nearly free: `display_text` already held the painted string, so `Some(_)` **was** the flag. The original contract test passes unchanged.
- [x] **O3.2 ☑ `ui.getAppliedStyles` — what the node is painted with.** The review narrowed this correctly: `get_styles` does **not** return the raw declaration, it returns the resolved cascade with origin and span. The real divergence is `apply_transitions`/`apply_keyframes`, which substitute the mid-flight blend into `css` *before* the split (`node_style` ← `css`, `node_computed` ← `resolved`) — so during a 300 ms fade `getStyles` reports the **target** colour while the node paints something else, and "why is this blue when my stylesheet says red" had no answer. A **separate method**, not an extra key: the `getStyles` response is a flat property map, so a sibling key would read as a CSS property to anything iterating it. Carries `animating`, checking *liveness* (`!committed`) rather than presence — a finished transition stays in the map until swept, which the first test caught. `snapshot`-gated, matching `get_styles`.
- [x] **O3.3 ☑ `ui.animations` + the stuck-animation check (W0116) — phase O3 complete.** `is_animating()`/`next_deadline()` existed on `Headless` and appeared **nowhere** in `lumen-agent`; `ui.waitSettled` uses the underlying condition without ever reporting *what* is moving, so an agent screenshotting mid-transition and diffing a golden could not know it caught a frame in flight. **Recalibrated per review:** W0116 fires only on a **finite** animation past its **own declared** duration — self-calibrating, no magic constant. An `animation: … infinite` is exempt by construction, because a spinner is doing exactly what it was told to for as long as it was told to; warning on elapsed time alone would fire on every slow-but-healthy fetch and double up with O4.4's resource-pending warning for the same non-bug. *Guarded by* `crates/lumen-agent/tests/animations.rs`, incl. a spinner running a full simulated minute with no finding.
- [x] **O4.6 ☑ `LogEntry` keeps a diagnostic's structure.** It was `{seq, level, message}`, and `Diagnostic::fmt` never printed the node — so flattening a finding into the ring destroyed the identity that makes it actionable, leaving the consumer to prefix-parse the code out of prose and guess the node from whatever backtick-quoted text each check's author happened to embed. Now carries `code`, `node` (the always-present **handle**, since an unnamed node has no `#id` at all; the friendly id stays readable in `message`) and `frame`, the cheap correlation primitive — entries sharing a frame came from the same pump, so "what happened when I clicked that" is a group-by rather than sequence arithmetic against a call you had to remember to make first. Free-text causal entries carry no `code` **by design** — that is the lint/log split working.
- [x] **O4.1 ☑ A press that reaches no handler says so.** `input.click` returns `{"ok": true}` whenever the *selector* resolved, regardless of whether anything was hit — so "I clicked it and nothing happened, and the tool said ok" had no trace. The routing walk already computed `did_click`/`did_focus`/`did_drag` and threw them away. **`input.click`'s return shape is deliberately unchanged** — agents and exported tests depend on it; the information goes to the ring.
- [x] **O4.2 ☑ A state write that a view depends on and that produced no frame.** "State changed but the UI is stale" is the top entry in the `debugging-lumen` skill and had no machine-readable trace, while `pump` computes every predicate involved and discards them. **Trigger narrowed per review:** the first draft would have warned on any write with no view dependents, which fires on every signal keying a `resource` — task deps live in `tasks.rs` and never register in `m.deps`, so the canonical async pattern has zero view dependents *by design*, and false positives would have been the first thing the audit ever logged in a real app. Condition is now "a view genuinely depends on it **and** the frame still went idle". New `Runtime::keys_written_since`, answered from the per-slot `version` the reactive graph already maintains. *Guarded by* `crates/lumen-agent/tests/causality.rs` — incl. the resource-key false positive, and a mechanism test standing in for the positive case, which only occurs on a genuine framework bug and cannot be synthesized without introducing one.
- [x] **O4.4 ☑ A failed fetch is reported even with no error UI — phase O4 complete.** `finish()` stores `Err(e)` on the resource cell *for the view to render*, and early in development a view usually renders neither error nor loading state — so a failed fetch was invisible and indistinguishable from one still in flight. Mirrored into the ring regardless. `E` is only bounded by `State`, so it need not implement `Display`/`Debug`; widening the public bound for a diagnostic would force every app's error type to satisfy it, so the message reports the type name (enough to locate the failing resource) and leaves the value on the cell.
- [x] **O4.5 ☑ Applied background results are announced.** `drain_deferred()` returned a count that `pump` discarded, so "the fetch never completed" and "it completed and the view ignored it" — two very different bugs — looked identical from outside. *Guarded by* `crates/lumen-widgets/tests/task_lifecycle_logs.rs`, whose failure test renders **no** error UI at all, which is the shape that made this invisible.
  *(O4.3 dropped — falsified rationale: `lumen-agent` hard-requires `lumen-widgets/snapshot` and `07 §3` states "a lean build implies no agent", so the lean-plus-agent configuration it existed to serve cannot exist.)* **O4.3 dropped** — its rationale was falsified: `lumen-agent` hard-requires `lumen-widgets/snapshot` and `07 §3` states "a lean build implies no agent", so the configuration it existed to fix cannot exist.
- [x] **O5.2 ☑ Text-cache thrashing is reported (W0117).** `sweep`'s own doc comment records the measurement — **1183 re-shapes per frame and a 2.2× frame-time penalty** (3.8 → 8.5 ms at 2000 rows) — and it emitted nothing, so an app in this regime simply got slower with no signal separating it from any other cause. Latched on **entry to the regime**, not per sweep: sweeps are routine, regime changes are not. `lumen-text` has no `Runtime` handle, so it reports upward via a new `TextEngineApi::take_diagnostics`, mirroring `Renderer::take_diagnostics` and the GPU backend's `atlas_overflow` latch — the review's claim that this pattern "does not generalize" to `lumen-text` did not hold up: it is the same trait shape one hop away. *Guarded by* two tests beside the existing thrash-policy ones: entering the regime reports once and stays latched, and a 50-row list with a ticking clock reports nothing.
- [x] **O5.3 ☑ Shell facts reach the ring — the O phase is complete.** Under `just run-agent` the agent reads a socket while stderr goes to the developer's terminal, so the renderer identity, present mode, the permanent direct→readback degradation, reload results and window failures were all invisible to the thing that most needs them. Renderer identity is emitted where `Headless` exists rather than beside its own `eprintln!` at startup — that site runs on the pre-`Headless` `App` builder, which owns no `Runtime` (the review caught this). `Present::Skipped` is throttled so a resize drag stays quiet and a sustained run does not. Split into two commits by subsystem rather than the four the review suggested — ~20 lines across sites that mostly share the `h.runtime().log(...)` shape.
- [x] **O1.2 amended — `ui.lastDamage` survives an intervening idle pump.** Found by driving a **live window**, which the headless suite structurally could not reproduce: a click that demonstrably repainted (`frames_rendered` 0 → 1, label `"0"` → `"1"`) reported `kind: none`, because the winit loop pumps continuously and an idle frame cleared `last_damage` between the action and the query. Now reports the last damage that actually *painted*, plus its `frame` so a caller can tell fresh from stale.

*Out of scope, recorded:* a frozen pump loop. Everything here instruments content **inside** a running frame; if the loop stalls, the JSON-RPC call hangs too, so no self-report from inside is possible. Needs an out-of-band watchdog.
## MOD7 ☑ The swap seams become reachable from a windowed app (2026-08-24)
`docs/plan-mod7-reachable-seams.md`. Lumen had seven swap axes, each with a second implementation and a test proving the bundle is consulted — and almost none of them could be reached by an app that opens a window. Auditing them by trying to *use* one found three defects; MOD7 fixes all three and adds the axis that was missing entirely.

**The prize, measured on two windowed binaries differing only in `PlatformConfig::Text`: 15.96 MB → 10.09 MB, 5.87 MB.** With LTO the linker drops parley, swash, skrifa, harfrust, the ICU tables and the embedded fonts once nothing instantiates the default engine — larger than LN3's 3.62 MB feature split, and previously headless-only.

* **S0 — the builder kept dropping the platform.** `with_renderer`/`with_executor` were typed `-> App<R2, E>`; the third parameter was absent, so it defaulted. A custom bundle reverted to taffy + parley the moment either was called, and it *compiled* — the mismatch surfaces only if the caller annotates the result. Two type annotations, no body changes. The guard observes the engine driving layout, because "does it compile" cannot catch it from the framework's side.
* **S1 — the shell pinned every axis.** `run`, `RunExt`, `Shell`, `SecondaryWindow`, `route_at_action` and `fulfill_system_requests` are now generic over `P`, defaulted so every existing `App::new(..).run(..)` is unchanged. **lumen-agent had to follow** — twelve signatures were fixed at `Headless<R, E>`. That did **not** show up in `cargo check --workspace` or the workspace tests, because the `agent` feature is default-off; the live-window gate caught it, which is the gap the `executors` leg exists to cover and did not.
* **S2 — one config for all four axes.** `AppConfig` + `ConfiguredApp<C>`, deliberately **additive**: `App<R, E, P>` keeps its three parameters, because varying one axis is a real use and a fused parameter would force a whole new config to change one thing. Renderer and executor arrive through **factory functions**, not a `Default` bound — `Box<dyn Renderer>`, the shape the shell itself uses, cannot implement `Default`. Also lands S1's deferred half: `run_with` honours a caller-supplied executor, so a windowed app can finally run on tokio or smol.
* **S3 — tuning as data, not more traits.** `PlatformConfig::TUNING` carries the cache ceilings. Only genuinely per-app caches are exposed: the glyph cache is `thread_local` and the image/animation caches are process-global statics, so a knob for them would read as configuration and behave as a race. **The method was first written into `TextEngine`'s inherent impl rather than its trait impl** — everything compiled, the runtime called the trait method and got the no-op default, so tuning reached nothing. The spy-engine test could not catch it (a spy *overrides* the method, proving the runtime calls it, not that the real engine obeys it); a dead-code warning did. Both tests are now present and the file records why neither is sufficient alone.
* **S4 — `presets::{Lean, Balanced, Desktop}`.** All three name the shipped engines and differ in the choices *around* them, because there is no second engine to name. Guarded by a test that two presets lay the same view out identically: presets trade memory and threads, not correctness.
* **S5 — declined**, `docs/mod7-s5-builder-decision.md`. A sixth entry point cannot remove the type declaration that is the only remaining cost (associated types need a named type — Rust's requirement, not the API's), and the value-shaped alternative would put the text engine behind `Box<dyn TextEngineApi>`, spending the 5.87 MB and a hot-path vtable hop on syntax. Revisitable if a second engine ships, or if the ~10-line `impl AppConfig` proves to be a real barrier — in which case a derive macro addresses it more directly than a builder.

**Not attempted:** the state store stays non-swappable. MOD6 measured it at **+117.6% on signal writes**. The precedent that matters is procedural — each seam is measured before it is promised.

## A11Y1 ☑ Accessibility becomes an opt-out feature — 2.15 MB (2026-08-24)
The OS accessibility bridge was an **unconditional** dependency in three crates. `LUMEN_A11Y=0` / `NO_AT_BRIDGE=1` existed but are runtime switches, so the code, the D-Bus stack and the thread `accesskit_unix::Adapter::new` spawns all shipped whether or not anyone wanted them. `accessibility` (default-on) makes it a compile-time choice.

*Measured*, `lumen-lean-app` against an otherwise identical `lumen-noa11y-app`: **16.08 → 13.93 MB, 2.15 MB.** `cargo tree -i zbus` shows accesskit is the only thing pulling zbus into the shell, so the gate takes the whole atspi/D-Bus stack with it — the OFF binary contains no `accesskit`, `org.a11y`, `atspi` or `zbus` marker at all. Larger than the 1.42 MB a symbol survey predicted, because that counted `.text` only.

**It gates the PUBLISHER, never the tree.** `SemanticsNode` is Lumen's observability contract, not an accessibility detail: twelve modules read it — the agent protocol, `ui.lint`, `lumen-test`, `audit.rs`, `wcag.rs`, the Android shell — and exactly one publishes it to the OS. Gating the tree would break the other eleven. `semantics_json`, `semantics_elided` and selector lookup stay available in both states, which is what `tests/accessibility_gate.rs` pins: the same three assertions compile and pass with the feature on and off, because "turning it off is invisible to everything except the OS bridge" is a claim that has to be checked in the state where it could break.

**A dead dependency fell out of the audit:** `lumen-app` declared `accesskit` and never used it. Removed.

**Two measurement traps, both hit and both worth recording.** Building the ON and OFF variants in one `cargo build -p a -p b` reported *identical* sizes — Cargo unifies features across a single invocation, so the OFF app silently got the feature back. They have to be built separately. And the first "baseline" was wrong in the other direction: `lumen-lean-app` passes `default-features = false` to the shell, so it had *already* lost accessibility and was measuring 13.93 against 13.93. It now names the feature explicitly to be the ON side.

*No trait, deliberately.* A backend seam was designed and declined for now: publishing is a clean one-way interface, but action routing is not — an AT click arrives as `ShellEvent::AccessKit(accesskit_winit::Event)`, an accesskit-typed variant in the shell's own event enum, and the adapter needs `&ActiveEventLoop` plus a window handle at construction. A generic backend therefore needs either winit types in the trait or a pull-based `poll_actions`, and today there would be exactly one implementation — the one-implementation trap this codebase has already hit with `Prop<T>` and with `PlatformConfig` being headless-only. The feature is a prerequisite for the trait either way, since the trait would live under the gate.

*Also caught:* the disk preflight added with the pre-push hook refused a run at 16 GB free. That is the guard working, and it is the most likely explanation for the two unreproducible `executors` failures earlier in this session — that leg always builds fresh under different features, so it is the one that would fail first under disk pressure.

## MUT9 ✗ Compile-time dep masks — measured, and MUT8 already took the prize (2026-08-31)

**Closed on evidence, the S3 precedent.** MUT9's charter — replace runtime
read-recording with a compile-time field mask where all reads go through
derived accessors — was sized against R9's "read recording survives any
representation: 5.2% of a frame" (≈11.4 ns of the keyed store's 26.5 ns
read). That premise died with MUT8: the state-field accessor's recording is
one borrow + index + push, measured at **~2.0 ns/read** (`storelookup`:
79.2 µs closed vs 181.2 µs open over 50 000 reads). A perfect mask can
recover at most those ~100 µs per 50 000 reads — and a real view reads a
handful of fields per scope, not fifty thousand, so the frame-level prize is
below the measurement noise floor. The downstream `ReadSet` construction
(`snapshot_reads`) is proportional to reads collected with a cheap dedupe —
also nothing at real read counts.

**What a mask would still cost:** either trusted author declarations (the
exact silent-freeze footgun S2 exists to make unrepresentable) or a
type-level projection proof, which is a research project, not a phase.
Spending that complexity to chase <0.1% fails the series' own bar.

**Reopen if**: a profiled app shows collector pushes in its hot frame (the
signature would be `track_state_field` in a flat profile), or the mask
becomes free as a side effect of some future typed-view work. The
measurements and the bench arm are in place.

**The MUT program is complete: MUT0–MUT8 landed, MUT9 closed measured.**
Frame contract, final scoreboard (one changed row of 50 000, vs where the
investigation started): bound-value patch **~215 µs flat in N** with
semantics current for free (a broken ~90 ms full rebuild before MUT0);
honest structural rebuild **3.0 ms** (was 9.3); the decline cliff **5.5 ms**
(was 320); the 15 ms observer tax **0**; a state-field read **3.4 ns** (was
26.5). Idle remains 0. The recorded residue and the staged Element deletion
are in the MUT7 entry.

## MUT8 ☑ The state struct is the storage — 26.5 → 3.4 ns per read (2026-08-30)

**S3-deep, shipped.** `App::with_state(state, |cx, s: &S| …)` threads one
`#[derive(Reactive)]` instance into the view. The value lives in the struct;
the runtime holds one **value-less `()` version slot per field** — so every
consumer of the reactive machinery (ReadSets, scope memos, `take_written`,
the MUT1 binding index, `structural_reads`) works on field writes unchanged,
without one new code path. The instance lives in its own cell OUTSIDE the
store's `RefCell`, so a view holding `&S` across the build never contends
with the store's borrows; builds are pure and handlers run outside builds,
so the shared borrow never crosses a mutation.

**The derive now generates the full surface** (S1's signal accessor renamed
`field_signal`): `s.field(cx) -> &T` — one recorded read
(`track_state_field`: one borrow, one index, one push, plus scope
subscription when tracking) and a direct reference; `S::get_field(rt)` for
`bind!` closures and handlers; `S::set_field` / `S::update_field`, which
mutate through `with_state_mut` and bump the field's slot exactly as a
signal write (write-gen, written log, dirty subs, flush). `ReactiveState`
carries the field names; ordinals are declaration order.

**Reload is the user's proven iced recipe**: the instance rides in the
snapshot as `"__app_state"`; `install_state` adopts a staged value at boot
and `adopt_pending_live` swaps it live (then touches every field so
dependents re-run); a value that no longer deserializes falls back to the
initial with a `W0002` diagnostic; `#[serde(default)]` makes shape changes
survivable. View-local state stays in the keyed store (D1).

**Measured** (`storelookup`, N=50 000, collector open — a build's reality):
today's `signal(key).get(rt)` **26.5 ns/read**; the state-field accessor
**3.4 ns/read** — **87% of the store cost removed**, above R9's predicted
63% because the accessor also skips the per-read re-addressing `rt.signal`
pays even on a warm slot. Read-recording residue: **1.8 ns/read** (the
collector push), down from ~11 — which reframes MUT9 before it starts.
*Guarded by* three `reactive_derive.rs` tests: per-field scope granularity
(writing one field re-runs one scope, the sibling splices), a `bind!` via
`get_*` patching without a rebuild through the reverse index, and the serde
reload restoring a live snapshot with defaults for missing fields.

## MUT7 ☑ The display-list walk goes dense; the Direct engine is measured end-to-end (2026-08-30)

**MUT7a — the rebuild frame's last big engine pass, profiled then flattened.**
Probes split the 2.7 ms `build_display_list` at chunk N=50 000 into:
paint_order 121 µs, **partition 1 304 µs** (three hash ops per node — a depth
`HashMap` insert+get plus `node_style`/`meta` lookups, to route nodes that
mostly emit nothing) and **emit 1 706 µs** (~34 ns/node of per-node early-out,
since only ~28 of 50 197 rows are on canvas — R.3 already culls emission; the
*iteration* was the cost). Landed: depth as a dense slot-indexed array
(`Tree::slot_count`); the hidden/overlay/effects tests as **flags-SoA bits**
— `NodeFlags::{OVERLAY, CSS_HIDDEN, LAYER_FX}`, written at the three
`node_style` insert sites and at lowering, `alloc`'s flag reset covering
recycling; the offscreen cull decided from `tree.clip` (the final derived
clip the bounds walk writes) + `LAYER_FX`, so the everything-offscreen case
pays **zero hash lookups**; and the glyph intern's linear scan replaced by a
key index in `DisplayList`. *Measured:* partition **1 304 → 79 µs**, emit
**1 706 → ~200 µs**; frames: chunk N=50 000 **4 204 → 3 046 µs** (−28%),
scope N=10 000 D=8 **12 273 → 8 014 µs** (−35%), decline path
**7 260 → 5 525 µs** (−24%).

**MUT7b — the Direct-only engine, staged honestly.** The architecture is
already Direct end-to-end at the root (`App::view<V: Direct>`, O0.24) and
every widget lowers Direct (O0.23); what "Element deletion" still means is
(a) statement-form migration of ~180 authoring files and (b) rewriting
`build_node` to consume widgets instead of the `Element` IR — the true 1.0
break, R8-costed at ~5% of a changed frame. This stage **proved the path at
scale**: sparse gained `MODE=direct` — a 50 000-row statement-form root
(`Stack::column(|c| … c.child(Label::new(bind!(…))))`) — and it is at **full
parity on the patch path** (215 µs, bindings register through the
`@direct_bridge`, the equivalence guard and coherence oracle both pass) with
**build −4.7%** (122.8 → 117.0 ms at 50k) and lower RSS. `Stack` gained
`width`/`height` (a statement-form root needs the definite containing block
T2's deferred text relies on — an API gap this exercise found). The
remaining stage is recorded, not implied: migrate authoring files
incrementally via `Element::direct`'s boundary, then rewrite `build_node`;
nothing perf-critical waits on it — the transient per-node `Element` is the
~5%, and the staging-tree half of that is already gone in statement form.

**What remains O(N) on a rebuild frame, for the record:** the view phase
(authoring + store reads — the S-series' territory), splice bookkeeping
(`nodes_copied`), `paint_order`'s walk (121 µs), the partition/emit iteration
(~280 µs now), `propagate_disabled`, and the warm layout solve. The engine's
change-driven contract holds everywhere the change is *known*; these passes
are the residue of the parts that still discover it.

## MUT6 ☑ Semantics patch in place — the 15 ms observer tax is gone (2026-08-30)

**Measured first** (`benches/src/bin/semtime.rs`, kept as the arm): after a
one-row text patch at N=50 000, the next semantics consumer — an agent RPC,
an attached screen reader, `assert_view_coherent` — paid **15 049 µs**
(split: 8.5 ms tree rebuild + 6.5 ms elide projection), 70× the 218 µs patch
frame itself, because every text patch set `sem_root = None` and invalidated
the elided cache wholesale.

**Now the projections are patched in place.** A lazy per-tree path index
(slot → child positions, built once per tree instance) lets
`patch_semantics_label` navigate straight to the node in both the full tree
and the elided projection and update exactly the fields
`build_semantics_at` derives for a text change: the label plus, under
`dev-observability`, the dep union, ink and text metrics (the paint runs
first, so the side tables are already fresh). Bounds, states, value and
identity are untouched by a layout-neutral patch. Each projection falls back
to the old invalidation when it cannot patch — the tree is shared (a
consumer still holds an `Rc`, pinned by the new fallback test), the path is
stale, the slot is missing — and a slot absent from the *elided* index just
means the node is elided out. The serialized JSON doc cannot be patched and
is dropped as before. **`assert_view_coherent` is the oracle**: every
binding-suite test compares the patched projections against a fresh build.

**Measured after: 0 µs** — the semantics after a patch is simply already
current. The remaining per-consumer cost for an attached screen reader is the
AccessKit adapter's own tree conversion (outside the engine; noted, not
addressed). `sem_gen` still bumps per patch so agents' change detection sees
patched frames.

## MUT5 ☑ `bind_text_color` — the paint-only binding set grows (2026-08-30)

**Scoped by evidence, not by the plan's list.** The plan named colour,
opacity, transform, value and visual state. Opacity and transform have **no
element-level property to bind** — `Tree::set_opacity`/`set_transform` have
zero callers and both effects flow exclusively from `.lss` into compositing
layers — so binding them means designing new node-level paint properties,
deferred to the animations rework rather than half-built here. Widget values
(ProgressBar) are layout-coupled and already land correctly through the MUT1
scoped-decline fallback. Visual state is the restyle path's job. What
remained with real leverage: **text colour**, the missing half of the text
story (content patched since F3.5, colour still forced a rebuild).

`Element::bind_text_color(Dynamic<Color>)` (field in `RareEl`, so `Element`
stays 784 B — the `type_sizes` gate is why): isolated + retained like the
background, indexed through `BindingSlot::Color`, settled in
`settle_bindings_for_rebuild`, carried across splices, folded into the same
pump arms. A colour change rewrites only the glyph run's **brush** through
the MUT2 surgery — the run itself is reused from the shape cache (colour
never reshapes) — and has no semantic footprint, so MUT6 does no work
either: the cheapest patch in the system. `.lss` `color` still wins at
paint, exactly as over a static colour. `NodeMeta.deps` gains a `color`
bucket (`ui.getDeps.byProp.color`); the facade gate calls the new builder.
*Guarded by* `a_color_binding_patches_without_rebuild` (build count stays 1,
pixels change, coherence holds) and the MUT2 byte-equality oracle, whose
view now carries a colour binding through its patch rounds.

## MUT4 ☑ Layout's two hidden O(N) passes removed — rebuild frames −45…55% (2026-08-30)

**The diagnosis rewrote the plan.** MUT4 was scoped as "warm layout for
rebuilt spans" on R6's cold-cache evidence. Phase probes (added, measured,
reverted) on the chunk N=50 000 steady frame showed taffy's caching already
working — the 2 927 µs "layout" phase was: **taffy's rounding pass ~1 675 µs**
(an O(whole tree) walk on every `compute_layout`, dirty or not), **
`update_abs` ~996 µs** (O(N) absolute-position pass, plus one `Vec` allocation
per node from `taffy.children()`), and the actual warm solve **~170 µs**.
Confirmed by flipping `disable_rounding`: the solve fell from 1 845 to
170 µs.

**What landed.** Taffy rounding is off; `update_abs` applies taffy's `round_layout`
formula byte-for-byte — and getting there took the goldens: the first version
rounded the cumulative absolute (`x0 = round(abs)`), which drifts a pixel
whenever fractional locations accumulate, and **five doc-shot goldens
caught it**. Taffy rounds the RELATIVE location per node (origin = running
sum of `round(location)`) while sizes use the unrounded f32 cumulative
(`round(cum+size) − round(cum)`); replicated exactly, all 68 doc-shots are
byte-identical again. While there it
**prunes**: an entry whose unrounded absolute rect is unchanged keeps its
subtree (`AbsEntry { raw, rect, stamp }`); the arena walk now prunes on
`node_is_fresh` (the stamp) instead of its own rounded-value compare — which
also closes MUT3's subpixel hole (a parent moved 0.4 px can keep its rounded
rect while a child's rounded position shifts a pixel; unrounded comparison
descends). The MUT3 build-epoch survives as a debug invariant
(`fresh || !born_this_epoch`).

**The soundness hole the layout suite caught before it shipped:** "unchanged
rect ⇒ unchanged interior" is FALSE on the dirty path —
`dirty_subtree_relayout_touches_only_descendants` re-styles a leaf inside a
fixed-size panel: the panel's rect is unchanged while its interior reflows,
and the first pruner skipped it (`touched()` 3 → 0). Fix: `set_style` marks
the node and its ancestor chain (`dirty_up`, taffy's early-out shape); the
pruner never stops on a marked node. Spliced spans mark nothing, so the hot
path is unaffected. RTL: `mirror_rtl` rewrites rounded rects behind the
pruner's back, so the first mirror call disables pruning permanently —
RTL apps forgo the optimization, correctness first.

**Measured** (sparse, taskset, K=1): chunk N=50 000 **8 817 → 4 171 µs**
(−53%); scope N=10 000 D=8 **22 198 → 12 193 µs** (−45%); decline path
**10 962 → 7 260 µs**; patch frames unaffected (218 → 200 µs). Also from the
probes, for the record: `damage_between` is **2 µs** on a rebuild frame (the
prefix/suffix trim works) — the remaining rebuild-frame O(N) is the **view
phase (~3.3 ms, authoring + store reads) and `build_display_list`
(~2.7 ms)**, which is MUT5's target list.

## MUT3 ☑ The bounds walk prunes unmoved spliced subtrees — 50 197 visits → 712 (2026-08-30)

**The rebuild frame's flat O(live nodes) bounds/clip pass is now change-
driven.** The F2.2 walk read every live node's solved rect and re-derived its
clip (two hash lookups per node) even when 49 939 of 50 197 nodes were
spliced and unmoved. It is now top-down with subtree pruning: a node
**retained from a previous build** whose solved absolute rect equals its
stored bounds has an interior laid out purely relative to that rect — nothing
inside can have moved, every stored descendant bound and clip is still exact,
and the walk stops there.

**The freshness signal had to be real.** "Solved == stored" alone is unsound:
the arena recycles freed slots, so a freshly lowered node can inherit a freed
node's index — and `alloc` resetting bounds to `Rect::ZERO` almost saves it,
except a genuinely zero-rect node aliases that too. The arena now carries a
**build epoch** (`Tree::bump_epoch` per rebuild, `born[slot]` per alloc):
`born_this_epoch` distinguishes spliced from fresh in O(1) and recycling
cannot fool it. Scroll needs no special case — offsets are expressed through
layout (negative margins), so a scrolled span re-lowers and descends. One
narrowing, documented in place: a `.lss` state-part `clip` reaches the
hit-test tree when the node re-lowers, not from a spliced frame — restyle
never wrote `tree.clip` either, so that edge is unchanged.

**Probed (added, measured, reverted):** chunk N=50 000 K=1 rebuild frames
visit **712** nodes and prune at **195** chunk roots (was 50 197 visits); the
coherence oracle's fresh build correctly prunes zero (every node born this
epoch).

**Measured** (sparse, taskset, K=1): chunk N=50 000 **9 299 → 8 817 µs**
(−5.2%); scope N=10 000 D=8 (90 001 nodes) **23 359 → 22 198 µs** (−5.0%);
decline path 11 072 → 10 962. *A correction to R7's attribution:* the phase
labelled "bounds walk, 1 162 µs, irreducible" evidently bundled neighbouring
per-frame work — the loop this replaced was worth ~500 µs of it. The walk
itself is now effectively free; the remaining rebuild-frame O(N) costs are
the splice bookkeeping (`nodes_copied`), the display-list rebuild + diff on
rebuild frames, `propagate_disabled`, and layout — MUT4's territory.

## MUT2 ☑ Patch-driven damage — the patch frame is O(K) and flat in N (2026-08-30)

**The last O(live nodes) work on a patch frame was `paint()` itself**: a full
`build_display_list()` walk plus a full `damage_between` diff, run to discover
a change the patch engine had just made. Both are gone from the steady state.

**Mechanism.** `build_display_list` records each *bound* node's footprint —
`DlSlot { text_cmd, bg_cmd, ineligible }` — while emitting (`emit_pass` takes
the bound set). A patch frame then calls `paint_patched`, which rewrites
exactly those commands inside the retained `last_dl` **in place** (same index,
same count, nothing shifts — why in-place beat splicing): the glyph run is
rebuilt from the cached `shaped_run` exactly as emission builds it, written
over `runs[id]`, and damage is the union of the command's paint bounds before
and after — the same rect `damage_between` would have found. The render tail
(region raster + `overwrite_rect`, or surface present via `last_damage`) is
unchanged. **Fallback, not failure**: any node whose footprint is more than
its own command — ellipsized display string, editor caret/selection, text
decoration, a bg patch on a node that emitted no box, a stale frame size —
returns `None` and the frame takes the full `paint()`, which is always
correct; a partial rewrite before a bail is harmless because the fallback
rebuilds the list from scratch. A bound node with **no** entry emitted nothing
(hidden, or culled off-canvas by R.3) — its patch changes no pixels and is
skipped.

**Measured** (`sparse`, taskset, K=1, D=0):

| workload | MUT1 | MUT2 |
|---|---:|---:|
| bind N=10 000 | 467 µs | **217 µs** |
| bind N=50 000 | 1 491 µs | **218 µs** |
| chunkbind N=50 000 | 1 534 µs | **217 µs** |

**Flat across N** — the plan's frame-contract term "O(K bindings), no term
O(live nodes)" is now measured reality on the patch path. Cumulative over
MUT0→2 at N=50 000: broken F3 would have full-rebuilt (~90 ms-class); the
working patch path went 2 162 → 1 491 → **218 µs**.

*Guarded by* `patched_frames_render_byte_identical_to_a_full_render`
(binding.rs): three patch rounds across a deferred label, a fixed-box label
whose string grows and shrinks (vacated pixels must clear), and a background
toggle, byte-compared against a fresh full render of the same state (ADR-002
CPU determinism makes byte equality meaningful) — this catches both wrong
command surgery and compositor under-damage. Plus the full splice/fuzz suites.
Remaining on the patch frame: ~215 µs of eval + semantics invalidation +
culled region raster; the *rebuild* frame's O(N) passes (bounds walk, DL
rebuild) are MUT3's territory.

## MUT1 ☑ Reverse-indexed bindings; the decline cliff is gone — 320 ms → 11 ms (2026-08-30)

**Two structural fixes to the patch engine, both demonstrated before fixing.**

**1. The staleness scan was O(bindings).** Every pump ran
`ReadSet::is_current` over every binding (twice on patch pumps). Now the
runtime keeps a deduplicated log of written **and dropped** `SignalId`s
(`Runtime::take_written`, fed at all four version-bump sites plus the
evict-drop site, so the index path sees exactly what the scan saw), and the
app resolves it through `binding_index: SignalId → [BindingSlot]` — rebuilt
once per rebuild after the F3.6 carry-forward, re-bucketed on a patch whose
read set changed (conditional bindings). `is_current` stays the authority;
the index only narrows the scan. *Measured:* bind N=50 000 K=1
**2 162 → 1 491 µs (−31%)**; N=10 000 **529 → 467 µs**.

**2. One declining binding cost the whole view.** `patch_text_bindings` was
all-or-nothing, and its rebuild fallback went through
`settle_bindings_for_rebuild`'s `clear_view_caches()` — so a single label
whose new string measured wider dropped **every** span and forced a full
unmemoized rebuild. *Demonstrated with the new `chunkbind` + `GROW=1` sparse
arms (NOFILL, so the labels take the eager path where growth genuinely
declines):* **319 963 µs** at N=50 000, `nodes_rebuilt=50 197`. Now the
verdict is per binding — patchable siblings commit even when one declines
(their spans splice through the rebuild and keep the values) — and the
decliner evicts only **the chain of cached scopes whose spans contain its
node** (`evict_scopes_containing`: an ancestor walk against the span-root
records, which at settle time still describe the previous build). *Measured:*
**11 114 µs, `nodes_rebuilt=258`** — the honest one-chunk rebuild, 195 chunks
spliced, **29×**. This is the "child asks its parent for space" fallback the
plan promised, built from machinery that already existed.

**A pre-existing coherence bug, exposed by the new switching-signals test:**
no patch or settle commit ever updated the node's observability projection
(`NodeDeps.text`/`.background`), so a conditional binding that switched
branches left `ui.getDeps` and the semantics `deps` describing the old branch
— `assert_view_coherent` fails on exactly that frame. All four commit sites
now project the fresh dep keys. GROW note: under a definite root the growing
string **patches** — MUT0's deferred rule makes width growth layout-neutral
by construction — which is itself the design working; the cliff only exists
on the eager path.

*Guarded by:* `binding.rs` — decline containment (sibling scope's counter
stays at 1), per-binding commit (both values land in one pump), and the
re-bucketed conditional binding. The remaining bind-frame tail
(1 491 µs at 50k) is `build_display_list` + `damage_between` — MUT2.

## MUT0 ☑ The F3 rebuild trigger diagnosed — a T2 regression, fixed: 18 430 → 529 µs (2026-08-30)

**The term that fires is `write_changed && !structural_current`, and the reads
are in `structural_reads` because T2 dropped them there.** Probed (added,
measured, reverted): on every bound-write pump, `structural_current=false` and
`text_bindings_stale()=false` — the binding's reads had gone structural and
**no binding record existed at all**. The cause is a seam between two features
landed a week apart: T2's deferred-measurement branch (2026-08-29) bypasses
the eager text-sizing block where F3.5 registers `BoundText`, so `pending_text`
falls through to the F3.5 safety net, which converts the reads to structural
and retains nothing. The net's `debug_assert` would have caught it, but **no
test combined `bind!` with a deferred-qualifying label** (stretched, invisible
box, definite containing block — i.e. any label in a `width: 100%` column, the
shape `FILL=1` makes and virtually every real root has). The 2.7×-slower-than-
plain overhead was 10 000 isolated closure evaluations per frame whose product
was thrown away. Cross-check: `NOFILL=1` (deferral impossible) and the patch
path worked all along — 565 µs, `nodes_rebuilt=0`.

**Fix:** the deferred branch registers the binding too, with `deferred: true`.
A deferred label's box takes nothing from the glyphs — width parent-assigned,
height line metrics — so the patch check is **"still a single line"**, no
shaping at all; a replacement gaining a `\n` falls back to a rebuild whose own
deferral guard routes the multiline text down the eager path. Same handling in
`settle_bindings_for_rebuild`.

**Measured** (`sparse`, taskset, K=1, D=0, FILL):

| arm | N=10 000 | nodes_rebuilt |
|---|---:|---:|
| plain | 6 806 µs | 10 001 |
| scope | 5 341 µs | 2 |
| bind before | 18 430 µs | 10 001 |
| **bind after** | **529 µs** | **0** |

**35× on the broken arm; 10× under the best previous authoring.** At
N=50 000: **2 162 µs** (vs C1's 9 047 best), linear at 0.043 µs/node — the
remaining tail is precisely `build_display_list` + `damage_between` on the
patch frame, which is MUT2's target. The plan's ordering is confirmed by its
first phase. *Guarded by* two tests in `binding.rs` that fail against the
unfixed engine (verified by stash): a deferred bound label patches
(`build_runs` stays 1), and a newline replacement falls back correctly. The
BENCH2/sparse claim "F3 is broken and expensive" is hereby resolved: it was
broken by regression, not by design.

## MUT ☑ Investigation: the maximum-performance architecture — build once, mutate (2026-08-30)

**Investigation, no code change** — the user's mandate: absolutely maximum
performance, widgets built once and mutated, reloadability and agent
observability preserved, architecture/API changes on the table (`.lss`
fast-restyle expendable). Full design in `docs/plan-max-performance-2026-08.md`.

The consolidation of PROF1–R10/F2.x/F3.x/O0.x/C/S into one cost model yields a
target frame contract — **cost = O(K bindings) + O(dirty-layout subtree) +
O(damage) + O(structural churn), idle = 0** — and a finding that revises R7:
the bounds walk + display-list diff called “genuinely irreducible” there are
irreducible only under *rebuild* semantics, where the engine must discover
change by walking; under mutation semantics both are O(K). Confirmed in source
that `paint()` re-runs `build_display_list()` + `damage_between` on every
painted frame regardless of what the patch path already knows.

Program **MUT0–MUT9**, ordered by measured size: MUT0 diagnose the F3 trigger
(bind is 2.7× `plain`, `nodes_rebuilt=10001`, term never isolated); MUT1
reverse-indexed per-binding commit with component-scoped fallback (kills the
all-or-nothing abort at app.rs:4929); MUT2 patch-driven damage + retained DL
(~2.0 ms at 50k); MUT3 incremental bounds walk (~1.2 ms); MUT4 warm layout for
rebuilt spans (20× at D=8, R6✗); MUT5 generalized property bindings; MUT6
semantics patching (observability stays current instead of being invalidated);
MUT7 Direct-only engine, `Element` deleted (~5%, R8); MUT8 **S3-deep reopened**
— the mandate is the API-change justification S3's deferral was waiting for
(8.8% floor, serde reload per the user's proven iced recipe); MUT9 compile-time
dep masks. Hypothesis to verify per phase, not promise: sparse K=1 at N=50 000
from 9.2 ms to under 1 ms.

## C2 ☑ `For` — the materialized keyed collection (2026-08-30)

R10's gap: `VirtualList` is the answer whenever the list is in a scroll viewport
(O(1) in item count), but some lists must materialize every item — not inside a
viewport, or something must reach items below the fold (find-in-page, Tab
traversal, an agent selector addressing a row by id). Those are the only case
the chunking curve still governs, and until now the author had to hand-roll it.

`For::new(cx, id, items, render)` chunks the list into memo scopes of **256**
and rebuilds only the chunks whose items changed. **The widget picks the grain,
not the author** — the whole point of R10.

**Measured** (N=50 000, one item changed):

| mode | frame | rebuilt | nodes |
|---|---:|---:|---:|
| plain | 54 565 µs | 50 001 | 50 001 |
| per-row `scope` | 42 139 µs | 2 | 50 001 |
| hand-written `Component` chunking | 9 157 µs | 258 | 50 197 |
| **`For`** | **9 148 µs** | 258 | 50 197 |
| `VirtualList` (where applicable) | 1 522 µs | 37 | 37 |

Within 0.1% of the hand-written chunking it replaces, with identical
`nodes_rebuilt` — so the widget is ergonomics, not overhead. `CHUNK = 256` from
R10's curve, whose floor is broad (64–1 024 all within ~9–10 ms), so the
constant matters far less than being off the endpoints.

**Documented limitation rather than a hidden one:** chunks are **positional**.
Append and in-place edit cost one chunk; **inserting at the front shifts every
chunk and rebuilds the list.** Fixing that needs per-item nodes with stable
identity — the one-scope-per-item shape measured 4.7× *worse*. Per-item *state*
is unaffected: `cx.component(item_key, ..)` inside `render` keeps its
scope-local signals across a chunk rebuild, because component identity is its
key.

`tests/for_list.rs` pins both halves of the contract — every item materialized
(what separates it from `VirtualList`) and one change costing ≤1 chunk (what
separates it from a plain `column`) — plus an idle pump rendering nothing and
two lists not colliding.

*A test bug worth recording:* the first version shared one `static` counter
across tests that the harness runs in **parallel**, so they reset each other and
the first-build assertion read 256 instead of 1 000. It looked exactly like a
real memoization bug in `For`. Per-test statics fixed it.

## S3 ✗ Field-as-slot — measured, and deferred on the evidence (2026-08-30)

S3 was to remove the addressing and lookup that R9 sized at **8.8% of a frame**.
Measured before implementing; the cheap version buys **nothing**, and the real
version costs the plan's largest API change.

**The cheap version does not work.** `Runtime::signal_at` is public and takes a
*precomputed* hash, so a derive can hash a field path once instead of per read.
Measured (N=50 000, collector open):

| arm | µs | ns/read |
|---|---:|---:|
| `address+read` (today) | 1 274.9 | 25.5 |
| **`address+read` with the hash precomputed** | **1 279.1** | 25.6 |
| hash only | 4.3 | 0.1 |

**Saved: −4.1 µs, i.e. nothing.** Hashing is free; the 511 µs called
"addressing" is entirely downstream of it.

**What it actually is.** `intern_hashed` does `inner.borrow_mut()` then a
`HashMap<IdHash, SignalId>` get (`state.rs:1125`); `signal_at` borrows *again*
for `slots.contains_key`; `read_with` borrows a *third* time for the slot and
then downcasts. **Three RefCell borrows and two map lookups per read.** None of
that is removable while the value lives in a keyed store — the `SignalId` would
have to be cached per *(Runtime, field)*, which is inherently per-runtime state,
which is the store.

**So the real S3 is the deep change**: the state struct instance holds the
values, the runtime holds per-field version counters, and a read is a direct
field access plus `note_read`. That requires the instance to be threaded through
the view — `App::new(|cx, state| ..)` — the largest API change in the plan.

**Deferred, on these grounds:**
* The prize is 8.8%, and R9 already established that the state-struct work is
  motivated by **correctness and ergonomics, not speed**.
* S1 and S2 delivered that motivation in full: compile-time identity, and
  `deps` deriving itself so the silent-freeze failure mode is gone.
* The remaining 5.2% (read recording) survives *any* representation, so even a
  perfect S3 leaves most of the store cost in place.
* Against C1/C2's 79% and 6×, this is not where the next effort belongs.

**Reopen if** the view signature changes for another reason — at that point
field-as-slot is nearly free to add, and this entry has the measurements ready.

## S2 ☑ `Component::deps` derives itself — the required-deps footgun is gone (2026-08-30)

`Component: Hash`, and `deps` defaults to `hash_of(self)`. An author writes
`#[derive(Hash)]` — **std's derive, no Lumen macro** — and the dependency is
exact by construction: every captured field participates, so the default cannot
*omit* one.

C1 shipped `deps` as a **required** method for exactly one reason: a
hand-written `deps` that forgot a captured field is memo-hit forever and renders
frozen content, silently, and looks *fast* while doing it. S2 removes the
failure mode rather than documenting it.

**Verified first, not assumed:** `scope_impl`'s cache check is
`cached.deps == deps && cached.reads.is_current(rt)` — **both**. Explicit deps
and read-tracking are additive, so a component that captures nothing hashes to a
constant and still correctly depends on whatever it read. The whole design rests
on that and it was worth reading rather than believing.

**Override paths kept**, both exercised in the tests: a manual `Hash` that skips
a field which does not affect rendering (an `Rc<dyn Fn>` handler, an `f64` which
is not `Hash`), and `SIGNALS_ONLY` to state "no captured data" explicitly.

*Consequence:* `Hash` has a generic method, so `Component` is **not
object-safe** — no `Box<dyn Component>`. Costs nothing: components produce
`Element`, already the currency for heterogeneous children.

**A weak C1 test was found and fixed.** `signals_read_inside_build_are_tracked_
without_being_declared` asserted that a signals-only component *renders* — and
never moved the signal, so it never tested the claim in its own name. A build
that ignored reads entirely would have passed. It now reads a
`#[derive(Reactive)]` field (S1 × S2), moves it, and asserts the rebuild.

**Bench unchanged:** `sparse`'s `RowGroup` dropped its hand-written `deps` for
`#[derive(Hash)]`; `component` and `for` arms unmoved (9 484 / 9 497 µs).

*Fourth occurrence of the same test bug, now fixed at the root.* A `static`
counter shared across tests the harness runs **in parallel** produced a failure
that looked exactly like a broken memo (`left: 3, right: 1`) — the memo was
fine, as a probe showed (`builds=1, rebuilt=0` on every pump). The S2 tests use
a **thread-local**, which is isolated per test by construction rather than by
remembering to use a distinct static.

## S1 ☑ `#[derive(Reactive)]` — field-path keys into the existing store (2026-08-30)

`lumen-macros`. For each named field, an accessor keyed by its **compile-time
field path**:

```rust
#[derive(Reactive, Default)]
struct Counter { count: i64, label: String }
// Counter::count(cx) -> Signal<i64>    keyed ("Counter", "count")
```

The key is `(&'static str, &'static str)` — `Hash + Debug` and allocation-free
per ADR-021 — and namespaced by the struct, so two structs may both have a
`count`.

**No performance claim, and the plan's exit criterion was corrected before
implementing.** R9 measured addressing with *integer* keys, already the
allocation-free fast path; a field path is equally fast, not faster. **S1 cannot
move the 8.8%** — that is earned by S3, where the field becomes the slot. The
draft plan had asked S1 to show "the addressing arm dropping toward the
field-read floor", which it is structurally incapable of doing.

**What it does buy — the two bugs from this session's own benchmarks:**
* A `format!("r{i}")` key allocating per row per frame (ADR-021's anti-pattern,
  worth 5 ms of 36). Now a compile-time path.
* `cx.signal(k)` inside a scope silently addressing a **scope-local** slot
  rather than the intended one — which reported a *fast* number for a frame that
  never updated. The accessor uses `Runtime::signal` (rooted at `ROOT_ID`)
  deliberately, so a field reads the same slot from any scope.
  `tests/reactive_derive.rs` pins exactly that, reading inside and outside a
  scope and asserting both see the write.

**D2's requirement met:** a field dropped between reloads is still reported as
**W0002**. The generated key is the snapshot key, so `finish_restore` reports it
exactly as a hand-keyed signal would — where plain `#[serde(default)]` would
drop it silently. Pinned by `a_dropped_field_is_still_reported_on_restore`.

**Also added:** `impl ReadCx for BuildCx`, so an accessor takes `cx` rather than
`cx.runtime()`. `tracks()` is **false**, matching `Runtime` — a build captures
dependencies through the read collectors (`note_read`, unconditional), not the
effect-subscription path `tracks()` gates; returning `true` would subscribe
every build-time read as if it were an effect.

**Shape of this phase, made explicit:** the struct is a **key namespace, not
storage**. Its fields are never read and the compiler says so; the values live
in the store. S3 is where the field becomes the slot.

## S0 ☑ State-struct model — both blockers resolved, S1 ready (2026-08-30)

`docs/plan-state-struct-2026-08.md`. Phased S0→S4, each shipping alone.

**D1 — view-local state — RESOLVED.** iced has a single state struct too, and
its answer is a second, framework-owned tree. Verified in `iced_core-0.14.0`:
`widget::Tree { tag, state, children }` persists across frames and holds
widget-internal state (`text_input` cursor, `scrollable` offset, `button`
pressed) — never the user's struct; children are matched **positionally**
(`diff_children` truncates and zips by index, `tree.rs:92`) with
`iced_widget::keyed` as the escape hatch. **Lumen already has this**: a
`cx.signal` inside a scope is scope-local, namespaced by scope path — the same
structural keying. Decision: the keyed store stays as the view-local mechanism;
the struct takes app data only.

*Convergent evidence:* iced needed `keyed::column` for the same reason `For`
(C2) documents its positional-chunk limitation, with `cx.component(item_key)` as
the same escape. Two independent designs landing on positional-plus-keyed-escape
suggests the shape is right rather than idiosyncratic.

**D2 — hot reload — RESOLVED.** The migration is serde + `#[serde(default)]`,
reported working in production by the owner on an iced app across adding,
removing and changing fields. **Lumen already implements that pattern** for the
keyed store: `snapshot()` writes JSON by stable key; `load_pending()` lets a new
signal take its `|| default` closure (`#[serde(default)]` by another name); and
`finish_restore()` emits **W0002** for any snapshot key never re-attached. The
`State: Serialize + DeserializeOwned` bound is already there. *Requirement
carried into S1:* the struct restore must **keep W0002** — plain serde drops
unknown fields silently, which is strictly worse than today.

Sized honestly by R9: **8.8% of a frame**, a floor. The motivation is
correctness and ergonomics — compile-time identity, and `Component::deps`
getting a correct automatic default that removes C1's silent-freeze failure mode
— not speed. Orthogonal to C1/C2 (79% and 6×); adopting it *instead* of them
would trade 79% for 9%.

## R10 ☑ Granularity belongs to the collection, not the author — and that widget already exists (2026-08-30)

**Corrects the framing of R7 and C1.** Both argued from a scope-granularity
curve; neither asked whether the author should be choosing a point on it at all.

The curve is real (N=50 000, one row changed, `MODE=chunk` sweeping the grain):

| grain | frame | rebuilt | corresponds to |
|---:|---:|---:|---|
| 1 per row | 50 522 µs | 3 | "each `Row` is a scope" |
| 64 | 9 618 µs | 66 | |
| **256** | **9 098 µs** | 258 | the optimum |
| 8 192 | 19 061 µs | 8 194 | |
| 50 000 (whole list) | 91 110 µs | 50 002 | "`rows: Vec<Row>` is a scope" |

Both *natural* mappings — per element and per field — land on the curve's
endpoints, 4.7× and 10× off the optimum. **But so does `Component`**: C1 does
not supply a grain either; the benchmark hand-wrote the chunking and passed
`CHUNK=256`. The criticism levelled at a state-struct mapping applied equally to
C1 and was not stated that way.

**The answer is a collection that owns its own granularity, and Lumen has had
one all along.** `VirtualList` calls `render(i)` only over the visible window
(`lists.rs:319`), so view, build, layout and paint are all O(visible):

| mode | frame | rebuilt | nodes existing |
|---|---:|---:|---:|
| plain | 54 523 µs | 50 001 | 50 001 |
| per-row `scope` | 41 420 µs | 2 | 50 001 |
| `Component`, chunk 256 | 9 161 µs | 258 | 50 197 |
| **`VirtualList`** | **1 524 µs** | 37 | **37** |

**6× better than the best author-chosen grain, and O(1) in the item count** —
1 534 µs at N=1 000 and 1 529 µs at N=200 000, a 200× increase with 37 nodes
throughout. The residual 1.5 ms is fixed overhead for shaping and painting a
37-row window, not a scaling term.

**What this does and does not change.**
* *R7's diagnosis stands* — the four costs and their sizes are unaffected.
* *C1 stands, with a corrected scope*: `Component` is for screens and sections —
  coarse memo units, ergonomic `deps`, teardown-and-rewrite. It is **not** the
  answer for large collections, and the 79% headline was measured on a workload
  where the real answer was `VirtualList` all along.
* *The methodological error worth recording*: `sparse` was built on a flat,
  non-virtualized 50 000-row column — a shape the benchmark report had **already
  disclosed as unrealistic** (commit `adf1e03`: "no competent 10 000-row screen
  is built this way in any of these frameworks"). Having documented that, the
  next benchmark was built on it anyway, and two features were then justified
  against it. A disclosure in one document does not protect the next experiment.

**Open, and now the actual gap:** a *non-virtualized* keyed collection (`For`)
for lists whose items must all exist — not in a scroll viewport, or needing
find-in-page. That is the only case the chunking curve still governs.

## R9 ☑ The keyed store costs 14% of a frame; a state struct could remove 8.8% (2026-08-30)

Measures the performance argument for a `#[derive(Reactive)]` state struct —
"each field is a scope, reads are field accesses". `benches/src/bin/storelookup.rs`,
N=50 000, min of 40, collector **open** (recording only happens inside a build,
so a closed-collector number alone understates it):

| layer | µs | ns/read | removed by field paths? |
|---|---:|---:|---|
| addressing (key → `SignalId`) | 510.3 | 10.2 | **yes** |
| slot lookup + downcast | 288.6 | 5.8 | **yes**, if the field *is* the storage |
| read recording | 466.7 | 9.3 | **no** — the engine must still know what was read |
| plain field access | 3.1 | 0.1 | — |
| **total** | **1 268.7** | **25.4** | |

Against C1's 9 047 µs frame at N=50 000 / K=1:

* total store cost **14.0%**
* **removable by a state struct: 8.8%**
* read recording, which survives any representation change: 5.2%

**This corrects an earlier inference in this thread.** The ~30% figure quoted
when the idea was first discussed came from attributing R7's whole `view` phase
(2 736 µs) to store lookups. Measured directly, the store is 1 269 µs — about
half of that phase — and only 799 µs of it is representation-dependent. The
performance case is **8.8%, not 30%**.

*Caveat, and it points the other way:* the arms use `rt.signal(i)`, while a
build uses `cx.signal(i)`, which additionally folds the enclosing scope's
prefix for scope-local namespacing. Real in-build addressing is therefore
**higher** than 510 µs, so 8.8% is a floor rather than a ceiling.

**Consequence for the design.** The state-struct proposal's *performance*
argument is modest and does not compete with C1's 79%; the two are orthogonal
(C1 sets rebuild granularity, this sets how a dependency is named and read). Its
strong arguments are correctness and ergonomics, which this measurement does not
touch: identity becomes a compile-time field path (allocation-free,
collision-free), and `Component::deps` gets a correct automatic default,
removing the silent-freeze failure mode C1 had to make a required method to
avoid. Both bugs this session's benchmarks hit — a `format!` key (ADR-021's
anti-pattern) and a scope-local signal read silently addressing the wrong slot —
become unrepresentable.

## C1 ☑ `Component` — a screen is a type, and the unit of rebuild (2026-08-30)

R7's conclusion was that three of the four costs in a "one row of 50 000
changed" frame are **authoring granularity**, and that the framework offered no
construct steering anyone toward the good one. `cx.scope` is available at *any*
granularity, which makes per-row — the worst — the obvious way to reach for it.

`lumen-app/src/component.rs`:

```rust
pub trait Component {
    fn deps(&self) -> u64;                            // captured plain data
    fn build(&self, cx: &mut BuildCx) -> Element;
}
cx.component(key, c)                                  // one memoized subtree
```

**Measured**, N=50 000 with one row changing (`sparse`, `CHUNK=256`):

| mode | frame | `nodes_rebuilt` |
|---|---:|---:|
| plain (naive) | 54 844 µs | 50 001 |
| per-row `cx.scope` | 42 346 µs | 2 |
| hand-written chunking | 9 128 µs | 258 |
| **`Component`** | **9 047 µs** | 258 |

**6.1× over naive, 4.7× over per-row scopes, and within 1% of the hand-written
chunking it packages** — identical `nodes_rebuilt` and node counts, so the
abstraction is ergonomics rather than overhead. That equivalence is the point of
the `component` arm in `sparse`: if the trait had cost anything over the scope
it wraps, it would show there.

**Design decisions and why.**
- *A thin layer over `scope_with_deps`, deliberately.* It inherits splice-in-
  place, taffy-node reuse (F2.1) and read tracking rather than duplicating them,
  so there is no second source of truth about what the tree contains.
- *Teardown-and-rewrite, not reconciliation.* Cost is bounded by one component,
  which is exactly what the coarse granularity buys. Scope identity survives, so
  scope-local signals and running tasks are kept across a rebuild — only nodes
  are rewritten.
- *`deps` is required, with no default.* A component built from captured data
  whose `deps` omitted it is memo-hit forever and renders frozen content —
  silently, no panic, no diagnostic. One required line removes the failure mode;
  `SIGNALS_ONLY` covers the no-captured-data case.
- *Not `build(&mut NodeTree)`.* R8 measured the `Element` intermediary at ~5% of
  a frame against this change's 79%. They are separable, and a component builds
  `Element`s internally today with no loss.
- *Mutate-then-build needs no mechanism.* A component is a struct: construct it,
  mutate it, hand it over. The `Direct` path's constraint was never about that —
  it is about editing *already-lowered nodes*. The real constraint is
  heterogeneous child storage (boxing), which is cheap per component and was not
  per node.

*Supersedes* the planned `#[component]` attribute macro with
`PartialEq`-on-props (W.3): a trait needs no proc-macro, and an explicit `deps`
states the dependency rather than inferring it — the same argument F3 made for
declared bindings over inferred holes.

`tests/component.rs` pins the contract: build skipped while deps hold, re-run
when they move, signals read inside tracked without being declared, siblings
distinguished by key.

## R8 ☑ The `Element` intermediary costs ~5% of a frame — not the migration to lead with (2026-08-30)

**The `Direct` migration removed `Element` in principle and not in practice.**
Counted: 62 widgets, **10** with a hand-written `Direct` impl, of which **8**
still hand an `Element` to `write_tree`/`write_children`. **1** writes fields
directly. The other 52 use the auto-generated `@direct_bridge`
(`build() → Element → write_tree`). So **61 of 62 widgets still construct a
784-byte `Element` per node**, and every `NodeWriter` method takes one. This was
staged deliberately — the trait's doc says it makes *"every widget is `Direct`"
true before "no widget builds an `Element`" is* — but the second half never
happened.

**Measured** with the existing WT-EXP probes (`lowertime` / `lowerprobe`, one
arm per process, equivalence-checked by `lowered_eq`), 500 rows × 3 widgets
≈ 2 000 nodes:

| | via `Element` | direct | |
|---|---:|---:|---:|
| lowering time | ~1 220 µs | ~960 µs | **−21%** |
| bytes allocated | 8.90 MB | 7.26 MB | −18% |
| peak live | 7.42 MB | 6.93 MB | −6.6% |
| allocation **count** | 10 803 | 10 302 | −4.6% |

The count barely moves because `Element` is a stack value: the 784 bytes are
*moved*, not heap-allocated. The 18% byte saving is its inline `Vec`s/`String`s.

**In frame context** (lowering derived from R7's phase split as
`rebuild_inner − view − layout − bounds − sweep`):

| | scope mode, N=50 000 | chunk mode |
|---|---:|---:|
| lowering | ~9 760 µs | ~2 350 µs |
| 21% of it | ~2 050 µs | ~490 µs |
| **share of frame** | **~4.8%** | **~5.3%** |

**Removing `Element` across 61 widgets buys ≈5% of the frame. The
component/chunking restructuring measured 79%** (R7). The two are **separable**
— a component can build `Element`s internally and still get the whole chunking
win, so `build(&mut NodeTree)` is *not* a prerequisite for components. Element
removal is also not all-or-nothing: converting the hot widgets (`text`,
`column`) captures most of the 5% without touching all 61.

*Caveat:* the 21% is from a Label+ProgressBar+Button workload with styles.
Text-only rows have a thinner `Element`, so their saving is likely smaller. The
frame-share figures are derived, not measured end to end.

**Order set by this:** the component trait first, `Element` removal second and
independently.

## R7 ☑ The O(N) floor diagnosed — it is four costs, and three are authoring (2026-08-30)

BENCH2 found that changing **one** row of 50 000 cost 44 ms with
`nodes_rebuilt=2` and damage confined to a 64×20 rect. Diagnosed by phase
instrumentation (added, measured, reverted — no behaviour change).

**It is not one floor.** N=50 000, K=1, steady-state minimums, per-row
`cx.scope` vs the same rows grouped 256 to a scope:

| phase | per-row `scope` | `chunk`(256) | |
|---|---:|---:|---|
| view | 16 604 µs | 2 736 µs | −84% |
| layout | 13 258 µs | 2 724 µs | −79% |
| `sweep_dead_scopes` | 752 µs | 3 µs | −99.6% |
| bounds walk (F2.2) | 1 278 µs | 1 162 µs | −9% |
| paint | 2 185 µs | 2 039 µs | −7% |
| **frame** | **42 915 µs** | **9 223 µs** | **−79%** |

1. **`cx.scope_with_deps` costs ~0.28 µs per call.** Chunking does not reduce
   the signal reads — the outer loop still reads all 50 000 — so the 84% view
   drop isolates the scope call itself: (16 604 − 2 736) / 50 000 ≈ 0.28 µs.
   **Per-row `cx.scope` is an anti-pattern at scale.**
2. **The root flex re-solves across all its children.** Chunking turns one
   50 000-child container into 196 containers of 256, so taffy re-solves 196
   children plus one chunk instead of 50 000. This is the same flat-container
   effect measured in R6 (D=0 gained 27%, D=8 gained 20×): **nesting helps
   layout, flatness hurts it.**
3. **`sweep_dead_scopes` is O(scopes)** — 752 µs for 50 000 of them.
4. **Genuinely irreducible: the F2.2 bounds walk + paint's display-list diff**,
   ~1.2 + 2.0 ms, unmoved by chunking. **≈0.064 µs/node** — the real framework
   floor, 3.2 ms at N=50 000 against 42.9 ms today.

**So three of the four costs are escapable by authoring, not by engine work** —
and the framework offers no construct that steers an author toward chunking.
`For` (designed in the 2026-07-03 F3 entry, never built) is exactly that
construct. Chunk-size sweep shows a U-curve with a broad optimum around 256
(16→12.3 ms, 64→9.7, 256→9.2, 1024→10.0): small chunks pay scope overhead,
large ones re-lower too much.

**Two more benchmark bugs, both caught by the equivalence guard** (its second
and third catches — it has now paid for itself three times):
- `key(i)` was `format!("r{i}")` — 50 000 String allocations per frame, the
  exact anti-pattern **ADR-021** exists to kill. Worth 5 ms of 36. Now an
  integer key.
- Chunk mode read `cx2.signal(i)` *inside* the scope, which is **scope-local**
  (F1 namespacing), so it read an always-zero signal rather than the one being
  written. The guard failed the run rather than reporting a fast, wrong number.

**Revises the framing again.** BENCH2 called the floor "some O(N) pass". It is
mostly not a pass at all — it is per-row scope bookkeeping plus a flat
container, both of which the *author* controls. The engine's own floor is 14×
smaller than the number that prompted the investigation.

## BENCH2 ☑ A sparse-update arm — the workload every real frame has (2026-08-30)

`benches/src/bin/sparse.rs`. N rows of which only **K** change per frame, with
three modes isolating which mechanism engages: `plain` (top-level structural
read — the control), `scope` (`cx.scope_with_deps` per row — F1), `bind`
(`bind!` per row — F3). Reports `nodes_rebuilt`/`nodes_copied` (the O(changed)
meter) and `damage` alongside frame time, because "did it get faster" and "did
it stop re-lowering the tree" are different questions and only the second
diagnoses.

Built because **fwbench changes every row every frame**, which makes it
structurally blind to the three things Lumen relies on to be fast — scope
memoization, the F3 patch path, and taffy's per-node layout cache. All three
pay off exactly when *few* nodes change.

**Two defects in the benchmark itself, found while validating it** — both would
have produced confident wrong conclusions:
1. Rotating the changed row over all N walked it **off screen** after ~28 frames
   (a 600 px viewport holds ~28 of 10 000 rows). It reported `damage=none`: it
   was timing frames where nothing visible changed. Rotation now stays within
   the visible span (`SPAN` overrides).
2. The scope was placed *inside* the depth wrappers, so 8 000 of 9 001 nodes
   rebuilt anyway and memoization looked useless at depth. The scope must cover
   the whole row.
An equivalence guard now asserts each mode actually applies its update before
its number is reported — a mode whose binding silently never fired would
otherwise look fastest.

**Findings (N=1000, K=1, one arm per process):**

| D | nodes | plain | scope | speedup | nodes_rebuilt (scope) |
|---:|---:|---:|---:|---:|---:|
| 0 | 1 001 | 1 048 µs | 872 µs | 1.2× | 2 |
| 4 | 5 001 | 7 255 µs | 941 µs | **7.7×** | 6 |
| 8 | 9 001 | 15 375 µs | **1 440 µs** | **10.7×** | 10 |

1. **F1 memoization works, and works very well at depth** — 15 375 → 1 440 µs.
   F2.1's taffy-node reuse holds, so layout is *not* recomputed cold for spliced
   spans. **This revises R6's framing**: the "layout is cold every frame" claim
   came from fwbench, where everything rebuilds by construction. Where spans
   splice, taffy's cache already survives.

2. **F3 (`bind`) is broken and expensive.** 19 038 µs vs `plain`'s 7 147 µs at
   N=10 000 — **2.7× slower than the thing it replaces** — with
   `nodes_rebuilt=10001`, i.e. it does not avoid the rebuild at all. Confirms
   and sharpens the earlier finding; the trigger is still undiagnosed (Step 1).

3. **The real target: a residual O(N) floor.** With `nodes_rebuilt=2` and
   `damage=region(64×20)`:

   | N | frame | per node |
   |---:|---:|---:|
   | 1 000 | 837 µs | 0.84 µs |
   | 10 000 | 5 137 µs | 0.51 µs |
   | 50 000 | **44 476 µs** | 0.89 µs |

   **Changing one row of 50 000 costs 44 ms** — nothing rebuilt, nothing
   relaid-out cold, one small rect repainted. Some pass is O(total nodes) every
   frame regardless. Candidates: the F2.2 bounds/clip walk over every live node,
   semantics generation, damage computation, or the root flex container
   re-solving across all its children. **This is what Step 1 should diagnose** —
   it is a larger and better-founded target than either T4 culling or R6.

## R6 ✗ Incremental layout is available after all — taffy caches, Lumen discards it (2026-08-30)

**Investigation, no code change.** Findings only; the probes were reverted.

The 2026-07-03 decision recorded "Incremental layout is SKIPPED … layout is one
`taffy::TaffyTree::compute_layout` (ADR-004) that can't be partially re-solved
across disjoint subtrees". **That premise does not hold for taffy 0.14.**

- `compute_layout(node, available)` takes **any** node as a layout root.
- `mark_dirty(node)` walks **up the parent chain**, and stops at the first
  already-dirty ancestor (`ClearState::AlreadyEmpty` → "No need to visit
  ancestors").
- `dirty(node)` is literally `cache.is_empty()`, and `compute/mod.rs:186` does
  `cache_get(node, &inputs)` with an early return **per node**.

So taffy already implements per-node layout caching with upward invalidation —
the exact algorithm GTK's `queue_resize` uses.

**Lumen discards it every frame.** `lumen-layout/src/tree.rs:105,119` call
`new_leaf` / `new_with_children`, so a rebuilt node gets a *fresh* taffy node
with an empty cache. F2.1 retains taffy nodes only for memo-**hit** spans;
everything rebuilt starts cold, and in the fwbench workload everything is
rebuilt.

**Measured** (N=1000, `FILL=1`, one arm per process; cold = today's behaviour,
warm = same tree recomputed with nothing dirty, one-dirty = a single leaf
invalidated then recomputed):

| D | nodes | cold | warm | one dirty leaf | frame | layout share |
|---:|---:|---:|---:|---:|---:|---:|
| 0 | 1 001 | 172 µs | 36 µs | 126 µs | 2 157 µs | 8% |
| 4 | 5 001 | 5 468 µs | 268 µs | 377 µs | 8 528 µs | 64% |
| 8 | 9 001 | 11 861 µs | 495 µs | **590 µs** | 16 705 µs | **72%** |

Cold grows superlinearly with depth; warm grows linearly. **The depth penalty
is cold-cache cost, not per-node cost.** At D=8 a single dirty leaf costs 590 µs
against 11 861 µs — 20×.

**Shape, not just constant.** Today Lumen is 2 157 → 16 705 µs for 9× the nodes
(7.7×, superlinear). With warm layout it would be ~2 021 → ~5 131 µs (2.5×) —
sublinear, matching GTK's 775 → 1 603 (2.1×). This is the mechanism behind the
depth column where GTK most embarrasses Lumen.

**Two honest limits.**
1. *Flat containers gain little.* At D=0 one dirty leaf costs 126 µs vs 172
   cold — 27%, not 20×. Dirtying one child of a 1 000-child flex column forces
   that column to re-solve across all its children. The win is in **nesting**,
   and fwbench's flat shape is the worst case for it.
2. *This benchmark cannot show the win.* Every row's text changes, so every leaf
   is dirty and propagates to the root. Incremental layout pays off when **few**
   nodes change — which is every real frame and no fwbench frame.

**What it would take:** stop calling `new_leaf`/`new_with_children` for nodes
that did not change, and `mark_dirty` the ones that did. Both halves already
exist — `cx.scope` retains the build (F1/F2.2), taffy retains the layout — they
are on opposite sides of a seam that throws the layout half away.

Supersedes the layout half of **T4** (viewport culling): T4 caps at 37% by
skipping layout for offscreen nodes, needs the every-node rule relaxed, and only
helps a shape real apps do not have. This is larger, needs no rule change, and
no new authoring API.

## A11Y3 ☑ The agent-only node payload is feature-gated (2026-08-29)

The version of "cull the tree in release" that is actually correct: gate the
**payload per node**, never *which nodes exist*. Culling the tree in release
ships an a11y hole to the users who need it and hides it from developers
(A11Y2); dropping fields nothing but the agent reads costs a shipped app
nothing.

Gated behind `dev-observability` (default **on**): `SemanticsNode.{ink,
text_metrics, deps, type_name}`, their producers (`node_ink` /
`node_text_metrics`, `AUDIT_MIN_INTERVAL_MS`, `last_audit_ms`), and their
consumers (`ui.getDeps`, `ui.whatDependsOn`, `DepEntry`, W0104). Plus the one
that is actually on the hot path: **`NodeMeta.deps`** — 4 × `Vec<String>` on a
struct built for every node of every frame — and the `dep_keys()` call that
fills it for every *bound* node. Reactivity itself runs off the `ReadSet`s,
which stay in both states; only the human-readable signal names go.

**Measured.** One arm crate (`lumen-obs-app`) with a feature passthrough, built
in two separate `cargo build` invocations — Cargo unifies features within one,
the trap A11Y1 recorded.

| quantity | on | off | delta |
|---|---:|---:|---:|
| `SemanticsNode` | 432 B | 320 B | **−112 B (−26%)** |
| peak RSS holding a 100 000-node tree | 423 MB | 383 MB | **−40.6 MB** |
| …tree-attributable | 76.5 MB | 36.0 MB | −53% |
| frame, plain-text rows (fwbench N=10 000) | 6 943 µs | 6 948 µs | **none** |
| frame, all-bound rows (N=5 000) | 27 451 µs | 26 812 µs | −2.3% |
| binary, this change alone | — | — | −7.5 KB |
| binary, whole feature | 20 105 296 B | 19 878 224 B | −227 KB |

Read those honestly. **Frame time is the number that did not move**: removing
the two per-painted-node side-table inserts changed nothing at all, in the
normal config *and* in an adversarial one with a 20 000 px window painting ~830
text nodes — paint culls, so almost nothing reaches that line. The bound-row
arm moves 2.3%, small but consistent (every off run below every on run), and
some of it is the ambient audit rather than `dep_keys`. The RSS figure is
**VmHWM**, i.e. peak: it exceeds the 11 MB the struct arithmetic predicts
because a smaller node also makes the transient spike of growing a
100 000-element `Vec` smaller, and it is superlinear in N for the same reason
(1.7 MB at N=10 000). **This is a footprint change, not a speed one**, and the
earlier suggestion that it might buy frame time was wrong.

**The compiler found the real target.** Gating the `SemanticsNode` fields alone
is nearly free (7.5 KB, no frame time). What made it worthwhile were the
dead-code warnings that appeared *after* the first gate landed: `NodeMeta.deps`
written and never read, then `text_deps`/`bg_deps`/`class_deps` computed and
discarded. Neither was visible before the gate existed.

**Two pre-existing failures fell out.** Running the suite with the feature off —
apparently for the first time — surfaced `task_lifecycle_logs.rs`, whose two
tests assert log lines `tasks.rs` has gated since O4.4 while the tests never
were. Verified pre-existing by stashing this change and re-running at HEAD.
Fixed here. The off state is now green (139 suites) rather than merely
compiling, which is the state a gate has to be checked in.

**`observability_gate.rs` runs the same assertions in both states** — the AT
contract, the virtualization contract, and selector addressing all intact —
because the risk of a gate like this is not that it fails to compile but that
it quietly takes something a screen reader needs. Same shape as A11Y1's
`accessibility_gate.rs`.

## A11Y2 ☑ Virtualized lists tell assistive tech their real size (2026-08-29)

Found while answering a performance question — *could the semantics tree be
full in dev builds and viewport-culled in release?* — by measuring what the
tree already contains. A `VirtualList` of 100 000 items:

```
semantics contains 24 text nodes
mentions a total/count anywhere: false
```

So Lumen **already** culls the semantics tree wherever it virtualizes, in every
build including release, and has done since `VirtualList` shipped. The culling
is correct and is the entire reason the widget is fast. What was missing is the
*declaration*: nothing told an AT that the 24 rows were a window onto 100 000.
A screen reader announces "list, 24 items", and rows 25..100 000 cannot be
reached — a **wrong** answer, not a degraded one.

**Fix — the standard virtualization contract.** `SemanticsNode` gains
`set_size` / `position_in_set`, mapped to AccessKit's `size_of_set` /
`position_in_set` (`aria-setsize` / `aria-posinset`; both present in accesskit
0.24.1, verified). `VirtualList` declares `set_size` on the `Role::List`
viewport and `position_in_set = i + 1` on each row in `place`, so a row reports
its index in the *real* space (50 001 of 100 000), not among the materialized
few (3 of 24). `DataGrid` does the same on its `Role::Table` and rows. Both
constructors of `VirtualList` funnel through `place`/`viewport`, so neither can
drift.

Both fields are `Option` and live in the **rare** half of `Element`/`NodeMeta`,
so an ordinary node still carries a null pointer rather than two more words —
the O0.13/O0.14 layout work is not disturbed. The rare box is now allocated for
materialized rows, which is bounded by the *window* (~24), not the item count.

`tests/virtualization_contract.rs` pins both halves, which is the point: it
asserts the tree really is culled (`< 100` nodes for 100 000 items — if that
ever fails the perf claim went with it) *and* that the culling is declared. A
fourth test asserts an ordinary column declares neither, since a spurious
`set_size` misleads an AT exactly as much as a missing one.

**A second, worse hole surfaced while checking whether the count was
*actionable*.** Declaring "100 000 items" is a description of an inaccessible
control if nothing can move the window. Measured against `Scrollable` as a
working control:

```
Scrollable   End -> 700        keyboard-operable
VirtualList  End -> 0          NOT operable
DataGrid     End -> 0          NOT operable
```

Both virtualized widgets were **wheel-only** — no `focusable`, no `on_key`. So
virtualizing a list *removed* AT access rather than degrading it: a plain
column of 100 000 is navigable (slowly), a `VirtualList` exposed 24 rows and no
way to move. W3 fixed exactly this for `Scrollable` and did not propagate; the
key map is now one shared constructor (`scrollable::scroll_keys`) so it cannot
diverge again.

**Focus is stored as an id, so `focusable` alone is inert.** `move_focus` does
`focused_id = meta.id.clone()`, meaning the traversal finds an id-less
focusable node and drops it on the same line. The first version of this fix
"passed" only because the probe set `.id("vl")`; with the id removed it went
straight back to 0. Every scroll surface now derives a default id from its
state name (`scroll_id` -> `vl-scroll`), overridable by an author `.id()`.
**This also fixed `Scrollable`**, whose W3 tests passed only because they set
`.id("sc")` by hand — as shipped it could not be focused either.

`tests/virtual_keyboard.rs` sets no id anywhere, keeps `Scrollable` as a
control, and asserts the row is **realized** — `Row 99999` absent from the tree
before and present after — not merely that an offset changed.

**A11Y2c (done, same day): the AT can now drive the list directly.**
`route_at_action` handled `Click` and dropped every scroll action AccessKit
offers. It now honours ScrollUp/Down/Left/Right (ScrollUnit::Item|Page),
SetScrollOffset, ScrollIntoView and ScrollToPoint. `SetScrollOffset` is the one
that closes the loop opened by `set_size`: it is how an AT that knows there are
100 000 rows jumps to row 50 000 — a node that does not exist and therefore
cannot be targeted. `at_actions.rs` asserts exactly that end to end: row 50 000
absent, resolve, inject, present.

*The decision is pure.* `a11y::resolve_at_action(tree, target, action, data) ->
Option<AtCommand>` has no `Headless`, no window and no adapter, so it is unit
tested; the shell is left with injection, which is the only part that needs a
live app. Previously the whole thing sat in `lumen-shell` behind a winit event
loop and was effectively untestable.

*Every scroll becomes a `WheelEvent`,* never a direct state write — otherwise
an AT scroll would be the one scroll in the app that skips chaining, clamping
and momentum, and would drift from the pointer the first time any of those
changed. An AT cannot reach a state a user could not.

*The actions are derived from `ScrollInfo`,* not declared per widget, and only
for axes with a non-zero extent — advertising `ScrollLeft` on a vertical list
makes an AT report a control the user cannot operate. Same lesson as
`scroll_keys`: anything that must be remembered per widget gets forgotten.
`None` means do nothing, never guess: unknown target, unimplemented action, and
missing or mistyped `ActionData` are all refused.

**Why not the dev/release feature gate that prompted this.** Three
measurements said no. (1) Post-T2 phase split at N=100 000 — `view=21262
build=23763 layout=40007 paint=6579` µs — paint is **6%** of the frame, so
culling at paint time is not where the remaining cost is. (2) The other 83%
needs the nodes to *exist*, so a real win means not creating them, which is
what `VirtualList` already does. (3) Release is precisely where screen readers
run: gating the full tree to dev builds would ship the a11y hole to users and
hide it from developers. Qt and GTK cull paint while keeping the full tree
exposed to AT, and carry set-size/position-in-set for the virtualized cases —
which is what this task implements.

## O0.16 ☑ `Direct` becomes a tree, and the `Element`-removal case is re-costed (2026-08-28)

The prototype's premise was that widgets write straight into the retained tree
with no `Element`. It never got there, and the reason was one signature.

**`fn lower(self, ..)` cannot be called through `Box<dyn Direct>`** — `dyn
Direct` has no statically known size to move out of (E0161) — so a container
could only hold children whose concrete types it knew at compile time. The
prototype implemented `Direct` for three *leaves* (`Label`, `Button`,
`ProgressBar`) and its `begin_row` said the quiet part out loud: *"the child
widgets are known statically at each site"*. True of a hand-written composite,
false of every real view, all of which are `column(vec![…])` over a mixed list.

`self: Box<Self>` fixes it. Added with it: `Node = Box<dyn Direct>`, `node(w)`,
`Open::child(Node)` / `child_of<W>` / `children(Vec<Node>)`, and a `Column`
holding `Vec<Node>` — the first container in the prototype that describes a
*tree* rather than a leaf. *Guarded by* `direct_children.rs`: a mixed
`Label`/`Button`/`ProgressBar` list lowering with no `Element`, containers
nesting as ordinary children, and a descendant selector matching through a
dynamic child list (which proves the container is on the ancestor stack while
its children lower — the ordering the typestate guards exist for).

**And then the re-costing, which is the point.** With the gap closed the arms
are finally comparable on the shape real views have — 500 rows × 3 children,
one process per arm:

| arm | median | allocs/node | bytes |
|---|---:|---:|---:|
| element | 1195 µs | 4.32 | 8.74 MB |
| boxed (dynamic children) | **1118 µs (−6.4%)** | **4.52** | 7.15 MB |
| element, styled | 1692 µs | | |
| boxed, styled | **1588 µs (−6.1%)** | | |

*Byte figures corrected 2026-08-28: the counting allocator did not implement
`realloc`, so the default alloc+memcpy+dealloc double-counted every `Vec`
growth. Originally recorded as 14.73 / 13.13 MB. The allocation **counts** were
unaffected and stand.*

**Removing `Element` is worth ~6%, not the ~24% this branch's report claims.**
Both of that figure's premises are now false: it was measured against a
**1072-byte** `Element` (O0.13/O0.14 made it 784 without any migration) and
against **static-arity** composition, which avoids the per-child box. Boxing
dynamic children *adds* 0.2 allocations per node, and that eats most of the
byte saving — 11% fewer bytes and 6.4% lower peak, but a slightly worse
allocation count.

So the honest case for the migration is now a single-digit percentage against
~180 files and a breaking change to `fn build(cx) -> Element`. The obvious way
to make it worth more is to remove the per-node allocation the boxes cost —
bump-allocate the widget tree — which is a different piece of work and should
be costed before, not after, the migration.

*Caveat, stated because it cuts against the result:* the `Column` used in the
boxed arm is minimal where `Container` is not, so the boxed arm is doing
slightly *less* work than the element arm. If anything the true gap is smaller
than −6%.

## O0.17 ☑ The arena experiment — bump-allocating the widget tree (2026-08-28)

O0.16 left one question: is the per-node `Box` what stops `Element` removal
from paying? `benches/src/bin/arenacost.rs` answers it. Widgets are placed in a
bump region and handed back as a `Box` over arena memory; the global allocator
recognises arena addresses and skips their `dealloc` — **the destructor still
runs**, only the free is elided, so the measurement is not cheating on drop.
Everything else keeps the system allocator, so the only difference between the
`boxed` and `arena` arms is where widget nodes come from.

500 rows × 3 children, one arm per process, min of 3 runs:

| arm | min | vs element | allocs/node | bytes |
|---|---:|---:|---:|---:|
| element | 1131.6 µs | — | 4.32 | 8.74 MB |
| boxed | 1029.3 µs | −9.0% | 4.52 | 7.15 MB |
| **arena** | **944.3 µs** | **−16.6%** | **3.72** | 6.89 MB |

**The arena roughly doubles the win**, and the mechanism is exactly the
hypothesis: allocations per node fall from 4.52 to 3.72 — below the `Element`
arm's 4.32 — because ~one `Box` per node disappears. So the ~6% of O0.16 was
the boxing masking the structural gain, not the ceiling.

### The harness bug that nearly produced a 52% result

The first version of this experiment reported `element` at 2477 µs against
`lowertime`'s 1193 for identical work, and was bimodal (min 1479, median 2562)
— which would have read as a **−52%** win for direct lowering. The cause was
that its `GlobalAlloc` did not implement `realloc`, so the default
alloc + memcpy + dealloc turned every `Vec` growth into a full copy. The
element arm's vectors hold 784-byte `Element`s and grow by doubling; the boxed
arm's hold 16-byte pointers, so the bug hit one arm and not the other.

It was caught by requiring the **control arm to reproduce an
independently-established number** before reading any comparison. That check is
the only reason the figure above is 16.6% and not 52%. `boxcost` had the same
omission and its byte figures are corrected above.

### What this does and does not license

It does **not** make `Element` removal a decided question. Two costs are
outside the measurement:

* **The arena device is a benchmark device.** `Box` over arena memory works
  here only because a custom global allocator skips the free. Stable Rust has
  no `allocator_api`, so a production version needs something else — an
  `&'arena mut dyn Direct` with `lower(&mut self)` (which forces widgets to
  yield their data through `mem::take`), a `bumpalo`-style dependency, or typed
  per-widget arenas. Each has its own cost, and **bump arenas do not run
  destructors by default** while our widgets own `String`s and `Rc`s, so drop
  tracking is mandatory, not optional.
* This is the lowering path, not a frame. Extrapolating from the last
  whole-frame split (`view` + `build_node` ≈ 1480 µs of a ~2300 µs frame),
  −16.6% is on the order of **a tenth of a changed frame** — an estimate, not
  a measured frame delta.

## O0.18 ☑ Drop tracking is free — the arena win is real and shippable (2026-08-28)

O0.17's −16.6% came from a device that cannot ship: a `Box` over arena memory
with a custom global allocator skipping the free. The open question was what
the *stable* mechanism costs, because bump arenas do not run destructors and
our widgets own `String`s and `Rc`s — a frame's worth of leaked strings per
frame is not a trade, it is a bug.

**The stable mechanism.** `Box<Self>` is the only owning `self` type `dyn` will
accept on stable, and it always frees through the global allocator, so
ownership had to leave the vtable. `Direct` is now by-value and `Sized`
(`lower_owned`), with an object-safe façade `DirectDyn` implemented for
`Option<W>` that *takes* the widget out of the slot. That one indirection is
what lets the identical widget be reached through a `Box` **or** through a bump
arena — which `Box` alone cannot do. `Column<N>` and `Open::child`/`children`
became generic over `N: DerefMut, N::Target: DirectDyn`, so one container
serves both.

The arena is `&mut dyn DirectDyn` over `Option<W>` in a bump region, with an
explicit drop list: one `(ptr, drop_glue)` push per node, walked in reverse at
reset.

| arm | min | vs element |
|---|---:|---:|
| element | 1122.3 µs | — |
| boxed | 1035.0 µs | −7.6% |
| **arena, drop-tracked** | **907.8 µs** | **−19.1%** |

**Drop tracking costs nothing — it is net positive.** The shippable arena beats
O0.17's unshippable one (908 vs 944), because a drop list walked once is
cheaper than routing every node through the global allocator's `dealloc` and
its address range check. The concern that motivated this experiment was the
wrong way round.

*Guarded by* an assertion that runs before every timed arm: 1000 `Drop`-counting
values are arena-allocated, nothing drops before `reset`, all 1000 drop at
`reset`, and a second round proves `reset` leaves the arena reusable. Without
it a leaking arena would present as a *faster* arm — the failure mode this
number is most exposed to.

`arenacost.rs` is retired; `arenadrop.rs` supersedes it with the mechanism that
can actually ship.

### Where the `Element`-removal case now stands

−19.1% on the lowering path with a stable mechanism, plus the modularity
argument (one node vocabulary instead of two hand-synced ones, 32 fields copied
verbatim). Against: a breaking change to `fn build(cx) -> Element` across ~180
files at 1.0, and `Direct` is a harder authoring surface than filling in a
struct — which principle 6 (`third-party (and agent-written) widgets are
first-class`) makes a real cost, not a footnote.

Still not measured: what any of this is worth on a **whole frame** rather than
the lowering path in isolation.

## O0.19 ☑ What `Element` removal is worth on a whole frame (2026-08-28)

Every figure up to O0.18 measured the lowering path in isolation. This measures
lowering's share of a real `pump` at current HEAD, which converts −19.1% on
lowering into a frame number without another extrapolation.

Two figures per shape. The **ceiling** is what a frame would lose if lowering
became *free* — a hard measured bound, not a projection, and the more useful
number for a go/no-go: nothing about direct lowering can beat it. The second
applies O0.18's −19.1%.

| shape | view | build_node | ceiling | at −19.1% |
|---|---:|---:|---:|---:|
| flat 1000 rows, styled | 85 µs | 262 | 60.1% | **11.5%** |
| flat 4000 rows, styled | 304 | 1005 | 58.2% | **11.1%** |
| nested depth 8, definite sizes | 163 | 684 | 46.8% | **8.9%** |
| churn (every label a new string) | 183 | 8738 | 88.4% | *not applicable* |

**Call it 9–11% of a changed frame.**

The churn row is left in as a warning rather than a result. Its ceiling is
genuine — 88.4% of that frame really is `view` + `build_node` — but ~92% of its
`build_node` is parley shaping (see O0.12), which is irreducible whatever the
node representation is. Applying −19.1% there would produce 16.9% by pricing a
cost direct lowering cannot touch. This is the same shape that produced the
original "wins don't transfer" error, and it stays in the table so the next
reader meets the trap with the answer attached.

*Caveat:* O0.18's −19.1% was measured on 500 rows of `Column[Label,
ProgressBar, Button]`, a richer row than `buildphase`'s single text node. The
two shapes are not identical, so 9–11% carries that seam.

### The decision this supports

Direct lowering with an arena buys **9–11% of a changed frame**, against a
breaking change to `fn build(cx) -> Element` across ~180 files at 1.0, and an
authoring surface that principle 6 makes a real cost. For comparison, the
O0.6–O0.15 series moved the same frame **5568 → 2251 µs (−60%)** without
touching public API at all, and **L1 — still open — is a 77× effect** on nested
auto-sized layout.

On those numbers `Element` removal is not the next thing to do. It is a
credible thing to do *later*, and O0.16–O0.18 leave it fully costed rather than
speculative: the trait shape works, containers hold heterogeneous children, and
the arena mechanism is stable-Rust and drop-safe.

## O0.20 ☑ Never boxing at all — children as statements (2026-08-28)

O0.16–O0.18 removed `Element` but kept a box per node: containers held
`Vec<Node>` where `Node = Box<dyn DirectDyn>`, and the arena only moved where
those boxes came from. The erasure was there to make `column(vec![…])` accept
heterogeneous children.

It is not needed. Hand the sink to the builder and children become
**statements** rather than values — heterogeneity comes from control flow, and
no widget is ever a trait object, boxed or arena'd. All four arms, same
2501-node tree, min of 5 runs each:

| representation | min | vs `Element` |
|---|---:|---:|
| `Element` staging tree | 1116.4 µs | — |
| `Box<dyn DirectDyn>` children | 1038.9 | −7.0% |
| arena `&mut dyn DirectDyn` | 955.3 | −14.4% |
| **inline, never boxed** | **914.2** | **−18.1%** |

`Open::child_of` was boxing (`self.child(node(w))`); it now calls `lower_owned`
directly, so a widget whose type is known at the call site never becomes a
trait object and the call inlines. The erased path remains for genuine
`Vec`s of mixed children and nothing else pays for it.

**The comparison is only meaningful because the arms were made to prove they
build the same tree.** Each returns a preorder fingerprint of (role, depth) and
all four report `4277b25d4ffdb592`. That check found a real defect first: the
inline arm closed its root with `LayoutStyle::default()`, which is a flex
**row**, so it had been laying out a different tree and winning the comparison
on layout rather than on representation.

**What this changes about the removal case.** The gap between arena and inline
is small (955 → 914), which says the per-node allocation was most of what
boxing cost and the rest is the lowering work itself. So the ceiling for
removing `Element` is ~18% of lowering — call it **9–11% of a changed frame**
by O0.19's shares — and it is reachable *without* an arena, a `bumpalo`
dependency, or drop-list bookkeeping. That is a materially simpler design than
O0.18's, for the same win.

It also costs something real: a widget can no longer be held unbuilt in a
`Vec` and edited before it lowers, which is one of the four patterns
`composition_showcase.rs` demonstrates. Statement-form children and
value-form children are not the same expressive power, and the erased path is
what preserves the latter.

## O0.24 ☑ The authoring API — statement-form views (2026-08-28)

Children can now be **statements** rather than a vector, which is the change
O0.20 measured at **−18.1%** against the `Element` staging tree. It is additive:
nothing that compiled yesterday needs touching.

**`impl Direct for Element`** is what makes it additive. Every
`fn build(cx) -> Element` view in existence is already a view returning
something `Direct`, so the new entry point accepts all of them unchanged and
statement form can be adopted one call site at a time.

**`App::view<V: Direct>`, and `App::new` left alone.** Making `new` itself
generic was tried first and reverted: a view body ending in `.into()` has no
unique target type once more than one type implements `Direct`, and **~25 call
sites in this repo alone stopped inferring**. That is a source-breaking change
even though the signature accepts strictly more, and no amount of it being
"more general" makes it free. `App::new` is `App::view` with `V = Element`.

**`Kids` and `NodeWriter::write_body`.** A statement-form body runs *during
lowering*, not during view construction — that is the whole mechanism. Each
`c.child(w)` is written the moment the body reaches it, monomorphically, so a
list of *n* rows costs one node at a time instead of *n* at once.

**`Stack::column(|c| …)` / `Stack::row(|c| …)`** is the first container in this
form. Its modifiers are hand-written because `impl_widget!` takes a concrete
type and `Stack` is generic over its body closure.

*Guarded by* `direct_engine.rs`:
`statement_form_and_vector_form_agree` requires the two authoring forms to
produce **byte-identical semantics trees** — cheaper is only interesting if it
is also the same — and `statement_children_come_from_ordinary_control_flow`
holds the property that makes statement form a real replacement rather than a
restriction: loops, conditionals and helpers all work, and none of them
collects anything.

### What is left of `Element`

It is no longer the authoring type, no longer the only thing a view can return,
and no longer how a container reaches its children. What remains is its use as
the per-node parameter block that `write_leaf`/`write_children` take, and the
~180 files that still *choose* to use the vector form. Those are now a
migration anyone can do incrementally, file by file, with both forms compiling
side by side — not a flag day.

## O0.23 ☑ Every widget is `Direct` (2026-08-28)

All **57** widget types implement `Direct` and lower through the sink. *Guarded
by* `every_widget_is_direct.rs`, which is a **type-level** check — nothing is
constructed, so it cannot be satisfied by a widget that merely happens not to
be exercised elsewhere, and a widget added without a `Direct` impl fails to
compile there.

**Two tiers, both correct.**

* **Bridged** (50) — `impl_widget!` now generates a `Direct` that builds the
  widget's `Element` and hands the tree to `NodeWriter::write_tree`. Every
  widget gets this for free, which is what makes "every widget is `Direct`"
  true before "no widget builds an `Element`" is. It is also what lets a
  converted parent hold *any* child monomorphically rather than through the
  boxed escape hatch.
* **Native** (7) — the containers: `Container`, `Card`, `Scrollable`,
  `Accordion`, `AppBar`, `PullToRefresh`, `Wrap`. Each grew a `parts()` that
  returns its node and its children *without joining them*; `Widget::build`
  puts them together, `Direct` never does. One construction, two consumers, so
  the paths cannot drift. `impl_widget!(Ty, native)` suppresses the generated
  bridge, so a widget cannot silently end up with both.

`NodeWriter::write_children` holds the context handling (a z-stack's absolute
positioning) in one place, so the next container to convert cannot forget it.

### What this does not yet buy, stated plainly

Native lowering here is **structural, not yet an allocation win**. A container
still receives its children as `Vec<Element>` from its constructor, so the
children are materialized either way — `parts()` only stops them being *joined*
into a tree field. The saving arrives when the authoring API stops handing
containers a vector, i.e. when children become statements at the call site
(O0.20's `inline` arm, measured at −18.1% against the `Element` staging tree).
That is the next stage, and it is the one that changes `fn build(cx) -> Element`.

*Guarded by* `direct_engine.rs::native_and_element_lowering_agree` — the two
paths must produce byte-identical semantics trees. `parts()` sharing one
construction is not sufficient evidence for that: the child lowering is
separate code on each path, which is exactly where a divergence would hide.

## O0.22 ◐ The `Direct` path is live in the real engine (2026-08-28)

The migration's architecture, complete and exercised against the engine rather
than against the prototype's model of it. Widgets can now convert one at a
time.

**Children became a callback.** `lower_node(el, parent, overlay, children)`
takes the subtree as `FnOnce` instead of reading `el.children`. This is the
inversion everything else turns on: while children were a `Vec<Element>` field,
a parent could not exist without its entire subtree existing first, so the peak
cost of a frame was the whole tree of 784-byte records alive at once. As a
callback they are lowered *while the parent is open* and never held — which is
also what makes context imposition (O0.21) expressible at all.

**`NodeWriter` erases `Sink`'s type parameters.** A widget has no business
knowing the renderer, executor or platform. Erasing them keeps `Direct` free of
type parameters, which is what makes the object-safe companion possible. The
dispatch is per *node*, not per field — one indirect call against everything a
node costs to write.

**The two tiers, as agreed:** `Direct::lower_owned(self, ..)` by value —
monomorphic, inlined, never boxed — and `DirectDyn for Option<W>` as the escape
hatch for a heterogeneous collection.

**`Element::direct(w)` is the migration boundary**, and it is the piece that
makes this incremental rather than a big-bang rewrite. Without it a widget
could only convert once its parent already had, so the conversion would have to
start at the root and change the authoring API before a single widget moved.
`Rc<RefCell<Option<Box<dyn DirectDyn>>>>` because `Element` is `Clone` (the
scope memo clones cached subtrees) and a boxed trait object is not; the widget
is *taken* on first lowering, so a clone and its original share one slot and
exactly one can lower it — which is the invariant the memo needs, since a
cloned stub is lowered *instead of* the original, not as well as it.

*Guarded by* `direct_engine.rs`: a `Direct` widget holding a count rather than
children produces real addressable laid-out nodes, its children are parented
and ordered correctly, and it composes **inside an ordinary `Element` tree
between two ordinary widgets** — which is the boundary property the whole
incremental plan rests on.

### What remains

Honestly sized rather than implied: the 57 widgets convert to `Direct` (each is
independent now), then `fn build(cx) -> Element` becomes a statement-form
signature across ~180 files, then `Element` and its `Vec<Element>` children go.
Each stage is independently landable on a green tree, which it was not before
this entry.

## O0.21 ◐ Context imposition replaces child editing (2026-08-28)

The architecture agreed after O0.20: **context is the primary mechanism,
monomorphic statement children the default, type erasure the escape hatch.**
This is the framework-wide part that does not depend on removing `Element`.

**`Sink` — the destination, separated from the source.** `build_node` *was* the
only way to produce a node: the writes into the arena, the layout tree and the
meta table were fused with the reads out of an `Element` in one 690-line
function, so a widget could not write a node without first constructing an
`Element` to be read out of. `Sink` bundles the four disjoint borrows a write
needs and `build_node` is now a *client* of it. That is what makes the rest of
the migration incremental rather than all-at-once.

**Both hold-and-edit sites are gone.** A survey found exactly two in the whole
widget library, and both were context impositions wearing an edit's clothes:

* **z-stacks** walked `children` writing `position: absolute` + `inset` into
  each one. Now `Element::stacks_children`, applied by the lowering as each
  child is written (`Container::stack` and `widgets::stack` both).
* **the disabled wash** was a recursive walk over the built tree run by
  `Common::apply`. Now `element::mute_node`, applied per node using the
  `disabled_count` depth the lowering already tracks.

Both are **strictly more correct** as contexts, which was not the argument for
making the change but is the better one: the walks only reached children
already present in the vector at the moment they ran, so a child appended
afterwards — or produced by a loop, or returned from a helper — was silently
missed. A context reaches every child by construction.

One test moved rather than broke: `third_party_widget` asserted the dimming on
the intermediate `Element`, which is where it used to be applied. It now
asserts it **on the painted pixels**, which is the actual contract; where in
the pipeline the wash happens is not.

Still open for the full removal: the `Sink` primitives a `Direct` widget writes
through (`Declaring`/`Open` over the engine's structures rather than the
prototype's), then the 57 widgets, then `fn build(cx) -> Element`.

## T1 + T2 ☑ Deferred text measurement — the Qt/GTK gap closes (2026-08-29)

The cross-framework benchmark put Lumen 2–9× behind Qt and GTK. Instrumenting
the shape cache found the cause was one thing: **10 010 shaping operations per
frame at N=10 000 against 19 cache hits**. Every label was shaped every frame,
including the 99% offscreen, because `build_node` sized each text node from its
shaped block. Bypassing shaping put Lumen at 6 928 µs against Qt's 6 433 —
**shaping was 87% of the frame and everything else was already competitive.**

**T1 — `TextEngine::line_height_for`.** A single unwrapped line's height is a
property of the font and size, not the glyphs, so it is answered once per
distinct `TextStyle` and cached: O(distinct styles), not O(nodes). *Guarded by*
`line_metrics.rs`, which requires **exact** equality with the shaped answer
across eight sizes — an approximation would move every baseline in the corpus —
and holds the claim the cache depends on, that a line's height does not vary
with the text on it.

**T2 — defer the measurement.** A text node is not shaped at layout time when
nothing consumes its intrinsic width. Two inherited bits carry CSS's
definite/indefinite distinction down the lowering: `cb_definite` (is the
containing block's width definite — true at the root, whose containing block is
the viewport) and `stretched` (does the parent assign this node's cross size —
a flex column with default stretch alignment does; a row does not, since width
is its main axis).

| N | before | after | Qt | GTK |
|---:|---:|---:|---:|---:|
| 1 000 | 5 711 µs | **2 220** | 3 275 | 775 |
| 10 000 | 52 847 | **7 369** | 6 433 | 7 294 |
| 100 000 | 568 614 | **105 192** | 53 467 | 77 119 |

Shaping per frame collapses from *N* to the visible-row count — what Qt and GTK
do. Peak RSS at N=10 000 falls 117 → 50 MB. **Lumen now lands level with both
at 10 000 nodes and ahead of Qt at 1 000.**

### The guard is the work

Applied without one, this fails **121 of 1 173 tests**. With the
definite/stretched guard alone it fails **one** — the `combobox` doc shot,
because leaving the width `Auto` makes the box span the parent instead of
hugging the glyphs. That is what CSS prescribes for a stretched block, and it is
still a *rendering* change wearing a performance change's clothes.

So the guard has a second half: a node is deferred only if its box is
**unobservable** — nothing fills it, outlines it, shadows it or clips it, and
the text sits at the start of it. With that, **1183 tests pass and no golden
moves**. *Guarded by* `deferred_text.rs`, which holds each half separately: the
height stays exact, a content-sized parent still measures its children, and a
label with a background, or centred text, or wrapping, keeps the box it had.

**Applies only under a definite-width ancestor.** A root that shrink-wraps gives
its children no definite containing block, so nothing defers — which is why the
benchmark's `FILL=1` variant exists. Whether Lumen's root should fill the
viewport by default is a separate question this raises but does not settle.

## L1 ☑ Nested auto-sized flex layout — 117× (2026-08-28)

Found 2026-08-27 while measuring O0.8, deferred behind the lowering work, taken
after it.

**It was taffy, not Lumen.** `benches/src/bin/taffydepth.rs` reproduces it in
pure taffy with no Lumen types, and the shape of the failure names the cause
exactly: leaf measurements of **2, 8, 32, 128, 512, 2048** at depths 0…10 —
`2^(depth+1)`. Each nesting level doubles the work below it, which is the
signature of a cache that is not serving the second pass rather than of one
that is thrashing. taffy's `Cache::get` keeps a single `final_layout_entry` and
a `ComputeSize` result cannot satisfy a `PerformLayout` request, so each level's
layout pass triggers a fresh cross-axis sizing walk with different keys.

Two facts narrowed it usefully before the fix:

* **Only columns.** Nested auto-width *rows* stay at 2 measurements per leaf.
* **Only the width axis.** A definite or percentage `width` collapses it to 2;
  `height`, `flex-grow`, `min-width` and an explicit `align-items: stretch` all
  leave it at 512.

**taffy 0.14 fixes it outright** — a constant 4 measurements per leaf at every
depth. The port is four lines: `min_size`/`max_size` moved from `Dimension` to
`LengthPercentageAuto`, and `lpa` already maps `Dim` identically, `Auto`
included.

Lumen frame, 100 rows, every node rebuilt, auto-sized nesting:

| depth | taffy 0.13 | taffy 0.14 | |
|---:|---:|---:|---:|
| 0 | 183 µs | 62 | 2.9× |
| 4 | 1 996 | 1 015 | 2.0× |
| 8 | 26 368 | 2 345 | **11×** |
| 10 | 103 645 | 2 875 | **36×** |
| 12 | 407 492 | 3 482 | **117×** |

**1162 workspace tests pass unchanged — no golden moved.** That is the evidence
that this is a performance fix and not a layout change, and it is why the
upgrade was preferred to the workaround below.

### The workaround that was prototyped and rejected

Rewriting an auto-width flex column to `width: 100%` during lowering gave 27× at
depth 8 and 288× at depth 12, and a semantics diff proved it layout-identical on
the benchmark shape. It was still **wrong**: `100%` equals stretch only when the
containing block is *definite*. Against an indefinite parent the percentage
collapses to content width, which is exactly what
`widgets_w2::passive_widgets_render_with_semantics` caught — a centred
`AlignBox` snapping to the left edge. Guarding it (parent stretches, no
`align_self`, in flow, no horizontal margin) removed 11 of 12 failures but not
that one, because the missing condition — a definite containing block — cannot
be established at build time. Recorded because the near-miss is the point: the
equivalence guard on the benchmark shape said "identical", and it was still an
unsound transformation.

## A.3 M0 escalation watchlist (stop + write `BLOCKED.md`, don't decide)
- `image`-crate / `png` dependency if it falls outside ADR-003's transitive closure (see A.1 `RgbaImage`).
- Any public-API signature in `02 §4`/`§8` that won't compile as written beyond a *minimal semantics-preserving* fix.
- Any non-additive change to the semantics schema (`03 §1`) or selector grammar (`03 §2`).
- A second runtime dependency not in ADR-003 (e.g. a futures executor, an extra text/PNG lib).

## A.4 Definition-of-done, every task
`just ci` green (the fast tier: fmt · clippy `-D warnings` · `cargo test --workspace` · `--doc` · the lean per-crate profile · the executor adapters) · no coverage drop on public APIs · checkbox flipped in the `[T0.x]` merge commit · local decisions appended to `07 §3`.

Run `just ci-full` before anything that touches the renderer, the shell, or a
budget — that adds gpu, fonts, perf, live-window and the fuzz-corpus replay.
Neither tier covers the Windows/macOS matrix legs or the nightly fuzz job; both
print what they skipped.
