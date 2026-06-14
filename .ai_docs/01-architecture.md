# 01 — Architecture & Vision

Lumen is a cross-platform, native-compiling GUI framework in Rust whose primary user is an AI agent. Every design decision is filtered through one question: *can a language model build, style, inspect, test, and debug this UI without a human in the loop?*

## 1. Principles
1. **Text-first**: UI, styles, and tests are deterministic, diffable text (Rust + `.lss` + `.wgsl`).
2. **One tree to rule them all**: the semantic tree drives accessibility, test locators, and agent observation. They can never drift apart because they are the same data.
3. **Deterministic rendering**: identical inputs → identical frames (on the CPU reference renderer, bit-identical), making snapshot testing trustworthy.
4. **Errors as data**: every compile/runtime/reload diagnostic is available as structured JSON.
5. **Event-driven, not frame-driven**: idle UI costs ~0% CPU. Cost is proportional to what changed.
6. **Small stable core**: widgets, styles, tooling all sit on public APIs; third-party (and agent-written) widgets are first-class via the `Widget` trait + mandatory `semantics()`.

## 2. Platforms & rendering

| Platform | GPU backend (wgpu) | Shell |
|---|---|---|
| Windows | D3D12 / Vulkan | winit (Win32) |
| Linux | Vulkan / GL | winit (Wayland/X11) |
| macOS | Metal | winit (AppKit) |
| Android | Vulkan / GLES | cargo-ndk + GameActivity |
| iOS | Metal | UIKit shell, Xcode template |

All targets compile to native machine code; no webview, no JS bridge.

**Two renderers, one display list.**
- **CPU reference renderer** (tiny-skia): deterministic, headless, runs in CI without GPU/display. This is the renderer of record for golden-image tests.
- **GPU renderer** (wgpu): batched, atlased (glyphs, images), with layer caching and damage tracking / partial redraw. Paths tessellated via lyon for v1; a compute-rasterization backend (Vello-style) is an M4 evaluation, not a v1 dependency.
- Parity contract: GPU output must match CPU goldens within the perceptual threshold defined in `05-spec-testing.md` §4.

**Hybrid tree + SoA hot data.** Widget logic and composition live in a tree of nodes (ergonomic, hierarchical — matches how styles cascade and events bubble). Per-frame hot data (bounds, transforms, opacity, clip, flags, z-order) lives in flat structure-of-arrays keyed by `NodeIndex` (`02-spec-core.md` §5). Culling, hit-testing, and damage aggregation are linear scans over packed arrays — ECS-grade cache behavior for bulk passes without ECS's structural costs for hierarchy. This is the resolved answer to "tree vs ECS for large UIs."

**Virtualization in the core.** Lazy list/grid/tree containers materialize only visible children; a million-row table costs a screenful. Live-widget count, not data size, bounds memory.

## 3. Layout & text
- Flexbox + CSS Grid + absolute positioning via Taffy, wrapped behind `lumen-layout` so the engine is replaceable. Incremental: dirty subtrees only.
- Text via parley (shaping/layout) + swash (scaling/hinting): bidi, fallback, emoji, variable fonts; IME and text editing are part of the core text stack, not a widget afterthought. CJK + bidi tests from the first text task.

## 4. Component model (summary; normative spec in 02)
- `Widget` trait with two archetypes: **composite** (implements `build()`, composes others — 95% of code) and **leaf** (implements layout/paint/event — primitives and third-party custom widgets). `semantics()` is mandatory for leaves.
- Fine-grained **signals** (no per-frame diffing): updating a signal re-runs only its subscribers. Derived `memo`s, `effect`s, async `resource`s.
- **State discipline (binding):** all retained UI state lives in the runtime's state store, keyed by stable identity; state types are `Serialize + DeserializeOwned`; **no closures, raw pointers, or non-registry trait objects in stored state**. Event handlers are re-registered on every `build()`. This discipline is what makes hot reload tiers 2–3, time-travel traces, and the future any-crate hot-patching linker (§7) possible.

## 5. Styling (summary; normative spec in 04)
`.lss` stylesheets: typed CSS-like language; selectors over type/id/class/state; design tokens; light/dark/high-contrast themes; implicit transitions + keyframes + springs; media/container queries; hot-reloadable with structured parse errors. Identical typed API available from Rust code. Styling-as-data is the agent's fastest iteration surface.

## 6. Custom shaders
WGSL `ShaderWidget`s, portable via wgpu: declared typed uniforms, built-ins (time, resolution, bounds, pointer, theme colors), composited and clipped inside the normal render graph. Two levels: fragment effects on a widget's rect, and full custom pipelines (M4). Shader hot reload with structured compile errors. Headless CI behavior: shader widgets render a deterministic fallback fill under the CPU renderer; correctness tests for shaders run on GPU runners.

## 7. Hot reload — three tiers
1. **Data (~one frame, zero risk):** `.lss`, `.wgsl`, assets pushed by the dev server file-watcher into the running app (desktop or device, same socket). Failed parse keeps the old version live + emits a diagnostic. State untouched.
2. **Code hot-patch (~0.5–2s):** components compiled into a `cdylib` behind a C-ABI registry of `build()` entry points; incremental rebuild, `libloading` swap under versioned names, affected components rebuilt. State survives because it lives in the store, not the component; changed state *shape* resets that component to defaults. Old dylibs are intentionally leaked (never unloaded).
3. **Snapshot restart (~2–5s):** for ABI-crossing changes — serialize state store + navigation/scroll/focus, full rebuild, relaunch, rehydrate.

Every reload emits a structured result event (tier, status, components swapped, state preserved, duration) on the agent protocol.

**Future track (separate project, out of v1 scope):** a Rust-aware hot-patching linker (function-level binary patching + dependency-graph-aware dylib reload, Subsecond/Live++ class). Lumen's obligation now is only to keep the **checkpoint protocol** (quiesce → serialize → resume) and the state discipline intact so that project can slot in as an upgraded tier 2. Recorded as ADR-014.

## 8. Tooling
- **`lumen` CLI**: `new`, `run --platform …`, `test`, `inspect`, `agent serve`; every command supports `--json`.
- **Emulators**: orchestrate, don't reinvent — Android Emulator via avdmanager/adb, iOS Simulator via `xcrun simctl`. Hot reload + agent protocol over the same dev socket on all platforms.
- **`lumen-test`**: Playwright-class harness — locators over the semantic tree, auto-waiting, input synthesis, text/state/layout/style/pixel assertions, trace recording. Spec in 05.
- **`lumen-agent`**: MCP server + JSON-RPC wrapping any running app — `get_tree`, `screenshot` (with optional ID-annotation overlay), styles/layout queries, logs/diagnostics, event subscription, and the same synthesized input as the test harness, plus `export_session_as_test`. Spec in 03.
- **Inspector** (M4): devtools app built in Lumen itself; everything it shows is also available as agent JSON.

## 9. Performance targets (CI-gated where marked)
- Idle UI: 0 frames rendered, <0.5% CPU. *(gated)*
- 120 fps capable desktop; 60 fps floor mid-range mobile.
- 1M-row virtual table scroll at full frame rate. *(gated benchmark from M2)*
- Layout of 10k-node dirty subtree < 2 ms desktop release. *(gated)*
- Cold start: <300 ms desktop, <800 ms mobile. Hello-world binary <5 MB.

## 10. Crate map
```
lumen-core      tree, NodeIndex/SoA hot data, signals, state store, events, semantics
lumen-layout    Taffy wrapper, incremental layout
lumen-render    display list, CPU (tiny-skia) + GPU (wgpu) backends, atlases, damage
lumen-text      parley/swash wrapper, editing, IME
lumen-style     .lss parser, cascade, tokens, animation scheduler
lumen-widgets   built-in widget library
lumen-shell     winit shell; android/ios shells (M3)
lumen-test      harness, locators, snapshots, traces
lumen-agent     JSON-RPC/MCP server
lumen-cli       dev server, hot reload, emulator orchestration
```

## 11. Milestones
- **M0** Foundations + verification tools (headless render, semantics, test harness seed, 10 primitives).
- **M1** Usable desktop framework (full text, `.lss`, 30 widgets, tier-1 hot reload, agent v1).
- **M2** Testing & AI loop complete (full lumen-test, tier-2 hot patch, traces, export-session-as-test, perf gates).
- **M3** Mobile (Android + iOS shells, emulator orchestration, touch/IME, mobile widgets).
- **M4** Depth (ShaderWidget, DataGrid/charts/rich-text editor, inspector, a11y certification pass, 1.0 freeze).

Detailed tasks and acceptance criteria: `06-task-graph.md`.
