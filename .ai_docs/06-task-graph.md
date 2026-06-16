# 06 — Task Graph & Acceptance Criteria

Topologically ordered. `Deps` are hard prerequisites. Acceptance = listed commands exit 0 in CI (Linux + Windows + macOS unless noted). M0 is fully decomposed; M1–M4 are decomposed to PR-sized tasks but with coarser acceptance — refine each into subtasks (recorded in this file) when you start the milestone.

Legend: ☐ open ☐→ in progress ☑ done. Update checkboxes in the task's merge commit.

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
`signal/memo/effect/resource` per 02 §4; identity-path keying; batched writes; subscriber-only invalidation; `Checkpoint` impl: snapshot → restore round-trip; `#[state_registry]` macro for stored trait objects; W0002 lenient deserialization.
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
**T2.3 ☑ Tier-2 hot patch.** cdylib registry, incremental rebuild orchestration, libloading swap, state-preservation, abi_hash downgrade to tier 3, intentional dylib leak. *Accept:* integration: edit a `build()` fn → swap <2 s on warm cache, counter state preserved; change state shape → that component resets, others preserved; core-crate edit → automatic tier-3 with state restore.
**T2.4 ☑ Tier-3 snapshot restart.** *Accept:* kill/rebuild/restore round-trip preserves signals, scroll, focus.
**T2.5 ☑ `session.exportTest`.** Recording, codegen to lumen-test source, auto-assertions. *Accept:* recorded session on settings app exports a test that compiles and passes.
**T2.6 ☑ Perf gates.** criterion benches: 10k-node dirty-subtree layout <2 ms; 1M-row VirtualList scroll ≥120 fps equivalent frame budget on reference desktop runner; idle = 0 frames. CI regression gate ±10%. *Accept:* bench workflow green + gate script.
**M2-exit ☑:** an agent connected only to `lumen-agent` can explore the settings app, export a regression suite, and the suite runs green in CI on 3 OSes.

---

## M3 — Mobile
**T3.1 ☑ Android shell** (cargo-ndk, GameActivity, surface lifecycle, touch, soft-keyboard/IME, safe areas). *Accept:* hello app runs on API-34 emulator in CI (headless emulator), agent screenshot matches golden perceptually.
**T3.2 ☑ Android orchestration** (`lumen run --platform android`: AVD provision, build, install, log stream, adb reverse for dev socket). *Accept:* scripted end-to-end on CI emulator incl. tier-1 hot reload.
**T3.3 ☑ iOS shell** (UIKit host, Metal surface, touch/IME/safe areas, Xcode project template). *Accept:* hello app on iOS Simulator (macOS runner) with agent screenshot golden.
**T3.4 ☑ iOS orchestration** (`simctl` boot/install/launch/screenshot; dev socket). *Accept:* scripted e2e on simulator incl. tier-1 reload; tier-2 verified on simulator, documented as tier-3-only on physical devices.
**T3.5 ☑ Gestures + mobile widgets** (GestureEvent full params; BottomNav, NavigationRail, AppBar, pull-to-refresh, DatePicker, TimePicker; touch target ≥44 px audit). *Accept:* gesture synthesis tests (pinch/pan/long-press) + widget test triples on both emulators.
**T3.6 ☑ `lumen test --platform android|ios_sim`.** *Accept:* M0-exit test passes unmodified on both.
**M3-exit ☑:** settings app runs on Android emulator + iOS Simulator; same test suite green on desktop+both; agent loop (edit `.lss` → reload → screenshot) works against the Android emulator.

---

## M4 — Depth & 1.0
**T4.1 ☑ ShaderWidget** (WGSL, typed uniforms, built-ins, CPU fallback fill, shader hot reload, E0201 diagnostics). *Accept:* GPU-runner goldens for 3 sample shaders; broken-shader edit keeps old pipeline + diagnostic.
**T4.2 ☑ DataGrid + Tree + charts + RichTextEditor.** *Accept:* test triples; DataGrid 1M-row gate added to perf suite.
**T4.3 ☑ AccessKit integration** (role/state map per 03 §1; platform a11y smoke tests). *Accept:* map table complete; VoiceOver/NVDA manual checklist documented + automated AccessKit-tree diff tests.
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

**T5.1 ☑ Web / WASM target.** wgpu→WebGPU with a WebGL2 fallback; a canvas-only shell (no DOM widgets); the CPU reference renderer compiled to wasm for golden parity; agent bridge over WebSocket/`postMessage`; asset/font streaming; wasm size budget. *Accept:* the settings + inspector apps run in headless Chromium, driven unmodified through `lumen-agent`, matching a perceptual golden; `lumen run --platform web`; wasm bundle under a gated size.
**T5.2 ☑ Desktop system integration.** Multi-window + multi-monitor (DPI/scale per window), native menu bar + context menus, system tray, native file/color dialogs, rich clipboard (text/image/files), drag-and-drop intra- and inter-app, OS notifications — all behind portable APIs surfaced on the agent + synthesizable in `lumen-test`. *Accept:* a multi-window app driven by the agent (focus, menu invoke, DnD between windows); clipboard + drop events synthesized headlessly in a test triple.
**T5.3 ☑ Internationalization & RTL.** Fluent-style message catalogs with structured missing-key diagnostics; ICU-class locale formatting (date/number/plural/currency); **RTL layout mirroring** in `lumen-layout` (start/end resolution, logical insets); per-locale theming; agent `input.setLocale`. *Accept:* one app rendered in en / ar / ja with RTL-mirror goldens; locale switch via agent reflows + re-mirrors; missing-translation surfaces as a `W####` code.
**T5.4 ☑ Navigation, global state, undo/redo, persistence.** Typed router with a back stack + deep links + guards; global stores layered on the signal runtime; a command/undo-redo history; whole-app state save/load (building on the Checkpoint protocol). *Accept:* deep-link navigation + multi-step undo/redo driven by the agent; app state round-trips through save→relaunch and through a tier-3 restart.
**T5.5 ☑ Forms & validation.** Declarative form state, sync + async validators, input masks/formatters, error→diagnostic surfacing with accessible error association (a11y `described_by`). *Accept:* a validated multi-field form; the agent fills it, reads validation failures as **structured data** (not pixels), corrects them, and submits.
**M5-exit ☑:** an agent, given only the CLI + lumen-agent, scaffolds and builds a **localized (RTL+LTR), multi-window, routed, form-driven CRUD app**, runs it on **desktop + web + the Android emulator**, exercises undo and deep-links, and exports a passing cross-platform suite from its own session — scripted as `examples/agent-gauntlet-web/`, added to the release gate.

---

## M6 — Media, Motion & Performance (rich, fluid, fast at scale)

*Theme: M5 makes Lumen deployable; M6 makes it **feel premium** and pays down
the GPU/perf debt deferred from v1 — rich media (vector/video/audio), a
world-class motion system, and the compute-rasterization + multi-threading work
flagged as a v1 evaluation. New ADRs: Vello-class GPU backend; media pipeline;
motion/choreography model.*

**T6.1 ☑ Vello-class GPU rasterizer.** A compute-shader path/scene rasterizer behind the existing display-list contract (selectable vs the lyon path); multi-threaded scene building; CPU↔GPU perceptual parity preserved. *Accept:* a complex vector scene matches the CPU golden within threshold on a GPU runner; path-heavy perf gate beats the lyon baseline; idle/damage contracts unchanged.
**T6.2 ☑ Vector & image media.** SVG rendering, Lottie/animated-vector playback, GIF/APNG, and jpeg/webp/avif decode with a shared image cache/atlas; declarative asset references resolved by the dev server (tier-1 hot-swap). *Accept:* SVG + Lottie goldens at fixed clock; codec round-trips; a swapped asset reloads live.
**T6.3 ☑ Audio / video / capture.** A media pipeline: hardware-accelerated video decode where available + a deterministic software path for CI, audio playback, and mic/camera capture, all clocked to the render loop. *Accept:* a video frame at a fixed timestamp matches a golden via the software decoder; capture surfaces are stubbable and agent-observable.
**T6.4 ☑ Motion system.** Physics springs, gesture-driven interruptible animations, **shared-element transitions** across routes, and a choreography/timeline API; the inspector's scrubber becomes a keyframe editor. *Accept:* gesture-driven + shared-element transition tests are deterministic under the virtual clock; choreographed sequence golden.
**T6.5 ☑ Advanced text & editing.** A real rich-text document model (styles, lists, tables, links, images), selection that spans widgets, find/replace, spell-check hooks, variable-font axis controls, and CRDT-ready edit hooks for future collaboration. *Accept:* rich-editor test triple; cross-widget selection + find/replace driven by the agent.
**T6.6 ☑ Performance at scale.** Multi-threaded layout, on-device GPU damage/partial redraw, a memory profiler + leak gate, and CI enforcement of the remaining `01 §9` budgets (cold start <300 ms desktop / <800 ms mobile, hello-world <5 MB). *Accept:* a 100k-node scene + all `01 §9` budgets gated in CI on the reference runners.
**M6-exit ☑:** a **media-rich, animated app** (video + SVG + shared-element navigation + a rich-text editor) holds 120 fps desktop / 60 fps mobile and passes every perf gate, agent-verified on desktop + both mobile emulators — added to the release gate.

---

## M7 — Ecosystem, Production & AI-Native (ship it; advance the thesis)

*Theme: everything required to **ship, distribute, extend, and trust** a Lumen
app in production — then the AI-native frontier the project exists for: an agent
that doesn't just build UIs but **operates** them (repairs regressions, imports
designs, certifies a11y) autonomously. Culminates in the 2.0 release. New ADRs:
distribution/signing; plugin ABI; the ADR-014 hot-patching-linker tier-2 slot.*

**T7.1 ☑ Distribution & packaging.** `lumen package` → per-OS installers/bundles (msix/dmg/AppImage/apk/ipa), code signing + notarization, delta auto-update, an asset-optimization pipeline, reproducible builds, and binary-size + supply-chain (`cargo-deny`/SBOM) gates. *Accept:* signed, installable artifacts produced per platform; the agent triggers a versioned release end-to-end.
**T7.2 ☑ Plugin & widget ecosystem.** Third-party `Widget` distribution over a stable ABI; `lumen add <widget>`; a Storybook-class component gallery app (self-testing); semver-checked widget APIs; doc generation. *Accept:* an external widget crate is installed and driven by the agent unmodified; the gallery drives every widget through its own self-test.
**T7.3 ☑ Production hardening.** Error boundaries + panic recovery scoped to UI subtrees, crash/diagnostic reporting hooks, opt-in privacy-respecting telemetry, a security review, and fuzzing of the `.lss`/agent/asset parsers. *Accept:* an injected panic is contained to its subtree and reported as a structured diagnostic (app stays alive); parser fuzz gate green.
**T7.4 ☑ Accessibility certification.** Real VoiceOver / NVDA / Orca driven in CI (not just AccessKit-tree diffs), a WCAG 2.2 AA audit with automated checks where possible, a11y of the inspector + agent themselves, and localized accessibility. *Accept:* screen-reader smoke tests pass in CI on 3 OSes; the WCAG checklist is automated where automatable and signed off where manual.
**T7.5 ☑ AI-native frontier.** An agent **auto-repair loop** (detect a regression → localize it via diagnostics + traces → patch → verify, unattended); the ADR-014 function-level hot-patching linker slotted in as an upgraded tier 2 (checkpoint protocol unchanged); design-import (Figma/Sketch → `.lss` + widgets) with agent reconciliation; self-describing components for agent authoring. *Accept:* the agent autonomously repairs an **injected functional regression** end-to-end with zero human edits; a design-import round-trips to a styled screen.
**M7-exit ☑ (2.0 release gate):** the grand gauntlet — an agent, given only the CLI + lumen-agent, **ships a complete production app** across all five platforms (desktop ×3 + web + mobile ×2): signed/notarized and installable, screen-reader-certified, localized (RTL+LTR), extended with a third-party plugin, with media + motion; it then **auto-repairs an injected regression** and re-ships — the entire pipeline green, zero human intervention, as `examples/agent-gauntlet-2/` and the 2.0 release gate.

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
- **Workspace:** virtual manifest at root listing all 11 crates + `lumen` facade (`02 §1`), `examples/`, `benches/`. Lockstep `0.0.0` version, `publish = false` for now. Shared `[workspace.dependencies]` table so every whitelisted crate version is pinned in exactly one place (satisfies ADR-003 "pin minor versions at repo init").
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
- `signal/memo/effect/resource` (`02 §4`), Solid-style fine-grained (ADR-007). Keying = identity path + `name`. A subscriber graph maps signal→scopes; writes mark only subscribed scopes dirty and are **batched** per loop turn; effects run after rebuild, before paint.
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

## A.3 M0 escalation watchlist (stop + write `BLOCKED.md`, don't decide)
- `image`-crate / `png` dependency if it falls outside ADR-003's transitive closure (see A.1 `RgbaImage`).
- Any public-API signature in `02 §4`/`§8` that won't compile as written beyond a *minimal semantics-preserving* fix.
- Any non-additive change to the semantics schema (`03 §1`) or selector grammar (`03 §2`).
- A second runtime dependency not in ADR-003 (e.g. a futures executor, an extra text/PNG lib).

## A.4 Definition-of-done, every task
`cargo fmt --check` · `cargo clippy --workspace -- -D warnings` · `cargo test --workspace` · `cargo test --doc` (rustdoc examples compile) · no coverage drop on public APIs · checkbox flipped in the `[T0.x]` merge commit · local decisions appended to `07 §3`.
