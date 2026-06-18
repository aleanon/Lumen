# Lumen — App-Framework Readiness: Gap Analysis & Plan

*Status: analysis as of 2026-06-18. Companion to `.ai_docs/06-task-graph.md`.*

## 1. The core finding

The task graph marks **M0–M7 complete (☑)** — including web, desktop system
integration, i18n, routing, media, distribution, and a11y. That is *true against
the milestones' own acceptance gates*, because nearly every criterion is written
to be satisfiable **headlessly, driven by the agent** ("driven by the agent",
"synthesized headlessly in a test triple"). Lumen optimized — deliberately and
successfully — for an AI building and verifying UIs without a human or an OS.

The gap to a **fully functional app framework that humans ship real desktop apps
on** is therefore *not* in the core, widgets, layout, styling, text shaping, the
CPU renderer, or the test/agent tooling — those are solid. It is concentrated in
the **real OS runtime (`lumen-shell`)** and the **content stack (fonts/assets)**:
rich portable APIs exist but are never wired to the operating system, and the
shell renders on the CPU.

> In short: **headless-complete, desktop-runtime-thin.** The portable APIs the
> agent exercises in tests are real; the bridge from those APIs to the OS is
> mostly missing.

## 2. Solidly present (do not redo)

- **Core**: tree + SoA hot data, fine-grained signals, state store, events,
  semantics (`lumen-core`).
- **Layout**: Taffy flex/grid/absolute, RTL mirroring (`lumen-layout`).
- **Styling**: `.lss` parser, cascade, tokens, transitions (`lumen-style`).
- **Widgets**: large set + `forms`, `nav`, `i18n`, `undo` modules.
- **Text shaping**: parley/swash — bidi, CJK, editing, IME *model* (but one
  bundled font; see §3).
- **Rendering**: deterministic CPU reference renderer (the contract); GPU
  offscreen backend exists but covers **only solid rects + image blits**.
- **Tooling**: `lumen-test` (strong), `lumen-agent` (JSON-RPC/MCP), `lumen` CLI
  (`new`/`run`/`test`/`package`/`agent`), inspector, hot-reload design.
- **Portable system APIs**: `lumen-widgets::system` (`MenuModel`, `WindowDesc`,
  `SystemRequest`, clipboard) — headless + agent-synthesizable.
- **New (this session)**: live-window agent endpoint in `lumen-shell`; design-
  analysis APCA contrast metric; resize correctness; paint caches.

## 3. The gap (evidence)

`lumen-shell/src/lib.rs` is a single-window, CPU-rasterize-then-blit shell:

| Area | Current state | Evidence |
|---|---|---|
| **Renderer abstraction** | None — the CPU renderer is hardwired in `Headless::paint`/the shell. The runtime should be **generic over a renderer** so tiny-skia (reference) and a GPU backend coexist and others can be added. | `paint()` calls `cpu::render_scaled` directly. |
| **GPU rendering** | Shell CPU-rasterizes the whole display list, uploads a full-window texture, blits it. The `lumen-render::gpu` backend is offscreen-only and supports just rects+images. | `Presenter::present` blits `h.screenshot()`; `gpu.rs` doc: "M0 implements … solid Rect fills and Image blits". |
| **Keyboard modifiers** | Hardcoded empty — no Ctrl/Shift/Alt in the live app. | `modifiers: Modifiers::empty()` ×3 in `window_event`. |
| **IME / text composition** | No `WindowEvent::Ime` handling in the shell (the *model* exists in `lumen-text`). | no `Ime` arm. |
| **DPI / HiDPI** | No `scale_factor` / `ScaleFactorChanged`; layout runs in physical px. | no scale handling. |
| **Native menus / context menus** | `system::MenuModel` exists + agent-invokable; shell never reads it. | no `menu` ref in shell. |
| **Clipboard / file & color dialogs / DnD / tray / notifications** | Portable APIs exist (`SystemRequest`, clipboard on `Headless`); shell wires none to the OS. | no `system`/`clipboard` ref in shell. |
| **Multi-window / multi-monitor** | Single `Window`; `resumed` early-returns if one exists. | `if self.window.is_some() { return; }`. |
| **Accessibility bridge** | Semantic tree is rich, but no `accesskit` platform adapter feeds it to OS screen readers from the shell. | no accesskit adapter in shell. |
| **Fonts** | One bundled font; `system_fonts: false`; no custom/system font loading; `TextStyle` is only size/weight/color. | `lumen-text` `system_fonts: false`, single `register_fonts(FONT)`. |
| **Robustness** | No error boundary / panic recovery around the live build/paint; no crash reporting. | event loop calls `pump()` directly. |
| **Hot reload** | Tiers designed; not wired into the desktop dev loop (file-watch → live swap in the running window). | — |

Web and mobile shells (`lumen-shell-web`, `-android`, `-ios`) are similarly
minimal; this plan focuses on **desktop first** as the proving ground, then
parity.

## 4. Plan

Phases are ordered by "what blocks shipping a real app." Each task keeps the
project's discipline: a portable API surfaced on the agent + synthesizable in
`lumen-test`, plus a golden/semantic acceptance.

### Phase A — A shippable desktop runtime (highest priority)

- **A1. Pluggable renderer + GPU surface backend.** Make the runtime **generic
  over the renderer** — a `Renderer`/`Surface` trait the shell selects at startup
  — so backends are *added*, never hand-swapped. **tiny-skia stays** as the
  deterministic CPU **reference renderer** (the golden contract, headless/CI) and
  a valid runtime choice; it is **not replaced**. Add a GPU surface backend
  alongside it: rect/quad pipeline (have), path/stroke tessellation (lyon — new
  dep, ADR), gradients, a glyph atlas, layer clip/opacity/blend. Leave room for
  future backends (e.g. a Vello-class compute rasterizer, T6.1) behind the same
  trait. *Accept:* one app renders identically (within the §4 perceptual
  threshold) under the CPU and GPU backends; backend is runtime-selectable; the
  CPU path stays bit-exact for goldens; GPU per-frame CPU encode in the tens of
  µs; resize stays crisp; idle/damage contracts intact.
- **A2. Complete desktop input. ✅ done.** `ModifiersChanged` → real modifiers
  (applied to pointer/key/wheel events); winit `Ime` events → `Preedit`/`Commit`
  into the text stack (`set_ime_allowed`); direct `KeyEvent::text` → `TextInput`
  when no IME is composing; key repeat already passed through. *Remaining:*
  visible focus-ring styling. (`map_modifiers` unit-tested.)
- **A3. DPI / HiDPI. ✅ done.** Runtime is logical-px; rasterizes at physical via
  `cpu::render_scaled` + `Headless::scale`/`set_scale`; shell reads
  `scale_factor`, derives logical size, converts pointer coords to logical, and
  handles `ScaleFactorChanged`. scale 1.0 stays byte-identical (goldens
  unaffected); layout/hit-testing stay logical (tested). *Remaining:* multi-
  monitor per-window scale (ties to A4 multi-window).
- **A4. Wire `system` to the OS.** Window title/min-size/fullscreen, real
  clipboard (text/image/files), native menu bar + context menus, native file/
  color dialogs, OS drag-and-drop, notifications — all behind the *existing*
  portable APIs. *Accept:* the same calls the agent synthesizes now open real OS
  dialogs/menus and read the real clipboard; a two-window app.
- **A5. AccessKit platform bridge.** An `accesskit` adapter in the shell that
  publishes the semantic tree to OS screen readers and routes a11y actions back
  through the input queue. *Accept:* Orca/NVDA/VoiceOver smoke test reads the live
  UI; the platform tree matches headless `semantics_json`.

### Phase B — Text & assets for real content

- **B1. Font loading.** Enable system fonts (fontique system backend) + a custom
  font registration API; family/fallback stacks; real weight/italic faces (drop
  synthesized-bold reliance). *Accept:* an app using a custom font + system
  fallback; CJK/emoji via system fallback render at parity.
- **B2. Rich `TextStyle`.** Family, letter/word spacing, line-height, decoration
  (underline/strike), per-run alignment. *Accept:* style goldens for each.
- **B3. Assets.** Image codecs (jpeg/webp/png/avif) + shared cache beyond the
  bundled path; declarative asset refs resolved by the dev server (tier-1 swap).

### Phase C — Dev loop & production robustness

- **C1. Desktop hot reload.** Wire the dev-server file-watcher to push `.lss`/
  asset/`cdylib` updates into the running window (tiers 1–2). *Accept:* edit
  `.lss` → live restyle with no relaunch; failed parse keeps old + emits a
  diagnostic.
- **C2. Error boundaries.** Panic recovery scoped to UI subtrees in the live
  shell + crash/diagnostic reporting (the model exists in `boundary`). *Accept:*
  an injected panic is contained; the app stays alive; structured diagnostic.
- **C3. Packaging hardening.** Turn `lumen package` into real signed/notarized
  per-OS installers + delta auto-update + size/supply-chain gates. *Accept:*
  installable signed artifact per desktop OS.

### Phase D — Premium & cross-platform parity

- **D1. Motion in the live shell** (springs, interruptible gestures, shared-
  element transitions).
- **D2. Media** (SVG/Lottie/video/audio) presented in the shell.
- **D3. Performance at scale** — multi-threaded layout, GPU damage/partial
  redraw, the `01 §9` budgets gated on real runners.
- **D4. Web + mobile shell parity** — bring `-web`/`-android`/`-ios` shells up to
  the desktop runtime's capability bar.

## 5. Sequencing & rationale

```
A2+A3 (input + DPI)  ──┐  small, immediately fixes the live feel
A1 (GPU renderer)    ──┼─► a desktop app that looks/performs native
A4 (OS integration)  ──┤
A5 (a11y bridge)     ──┘
        ↓
B (fonts/assets)  → real content
        ↓
C (hot reload, error boundaries, packaging) → dev velocity + ship
        ↓
D (motion, media, perf-at-scale, web/mobile parity) → premium + ubiquity
```

Phase A is the unlock: without it nothing reaches a human as a native app.
Within A, do **A2+A3 first** (days; they remove the most visible live-window
deficiencies — no modifier keys, blur/scale) then **A1** (the big one; biggest
quality + perf win and the thing that makes Lumen feel native).

## 6. Recommended first concrete step

**A2 + A3** (desktop input completeness + DPI) as one focused task on the live
shell — it's small, testable through the live-window agent added this session,
and removes the most glaring "this isn't a real app yet" gaps. Then commit to
**A1** (GPU surface renderer) as the headline effort.
