# Lumen — App-Framework Readiness: Gap Analysis & Plan

*Status: analysis as of 2026-06-18. Companion to `.ai_docs/06-task-graph.md`.*

> **Progress:** A2, A3, C2 (top-level), E3, and **A1 (the renderer abstraction)**
> are done; the whole iced-parity gallery is redesigned to the hero style. The
> remaining scope-deferred work (A1's GPU backend, B2/B3, C1, D1, E1, E2) is large
> and tracked in `docs/backlog.md`; sandbox-blocked items need real OS/AT/codec/
> signing/device infra to verify.

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

- **A1. Pluggable renderer + GPU surface backend. ◑ abstraction done.** The
  `Renderer` trait + `CpuRenderer` ship; `Headless` holds a `Box<dyn Renderer>`
  (`set_renderer`/`renderer_name`), so the runtime is now generic over the
  backend (tested by swapping one in). *Remaining:* the GPU surface backend
  itself (paths via `lyon` — ADR; gradients; glyph atlas; layers). Make the runtime **generic
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
- **C2. Error boundaries. ✅ done.** Subtree `error_boundary` already existed;
  added top-level containment — `rebuild()` catches build/layout/paint panics, the
  window keeps its last frame and reports `E0701`, clearing on the next clean
  build. *Remaining:* a crash-reporting *hook* (telemetry sink) — backlog.
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

### Phase E — Core ergonomics & type design (debt)

*Cross-cutting; not user-visible, but it shapes extensibility and the third-party
widget story. Can run in parallel with A–D.*

**Two non-issues to clear up first (they are *not* reasons to keep the flat
struct):**

1. **Clone cost is orthogonal — it's just E3 below.** No widget is ever cloned
   wholesale; the per-node field clones happen only because `build_node` *borrows*
   the soon-to-be-dropped `Element`. Consuming it (E3) turns those into moves
   regardless of whether nodes are a struct, an enum, or `Box<dyn Widget>` (a
   trait object relocates as a cheap pointer move).
2. **Diffing is not a live requirement, and a hash/`PartialEq` solves it.** The
   tree is **full-rebuilt every pump** (`rebuild()` makes a fresh `Tree`/`meta`);
   nothing diffs `Element`s today, so "the flat struct is diffable" buys nothing
   now. When memoization is wanted (skip rebuilding unchanged subtrees), the spec
   already prescribes the mechanism: `#[component]` *"PartialEq on props skips
   rebuild"* (02 §3). A trait can carry `PartialEq`/`changed(&prev)`/`state_hash`
   — retain only the prev hash/props, not the prev widget. (Prefer `PartialEq`:
   exact, no collision risk; hash data not closures; pair with stable keys for
   identity.) So a `Widget` trait loses no diff/memo capability.

- **E1. Slim `Element` / leaf-content enum.** `Element` is a 728-byte flat struct
  (256 B of which is `LayoutStyle`) that carries the *union* of every widget
  kind's fields — `text`/`image`/`canvas`/`scroll`/handlers — nearly all unused
  per node, and it lets illegal combinations exist (text **and** image **and**
  canvas at once). `NodeMeta` (retained per node) inherits the same width.
  Replace the mutually-exclusive leaf fields with a `content: NodeContent` enum
  (`Box`/`Text`/`Image`/`Canvas`/`Custom`) while keeping the common fields
  (id/role/style/children) flat. *Why this shape:* `Element` is a transient
  build-time *description* lowered into the compact tree+SoA, so the win is type
  safety + clone/alloc cost, not steady-state footprint; an enum keeps it
  `dyn`-free and simple to introspect. *Accept:* `size_of::<Element>()` and
  `NodeMeta` drop materially; invalid leaf combinations are unrepresentable;
  goldens unchanged.
- **E2. Implement the spec's `Widget` trait (`02 §3`).** Today there is **no
  `Widget` trait** — `02-spec-core.md` specifies an opaque `Element` + a `Widget`
  trait (composite `build()`, leaf layout/paint/event + mandatory `semantics()`),
  but the implementation exposes one public kitchen-sink struct and offers custom
  leaves only via the `canvas` closure. This blocks the stated 1.0 goal that
  "third-party (and agent-written) widgets are first-class via the `Widget` trait"
  (`01 §1.6`) and M7's plugin ecosystem (T7.2). Composites stay functions
  returning `Element`; leaves become `Widget` impls lowered to
  `NodeContent::Custom(Box<dyn LeafWidget>)` — a *transient build output*, so the
  "no trait objects in *stored* state" discipline holds (note `NodeMeta` already
  carries `Rc<dyn Fn>` handlers per build; that rule is about the serializable
  signal store, not the per-frame node tree). Memoization rides the trait via
  `PartialEq`/hash (see non-issue 2). The remaining real trade-offs are only
  vtable dispatch in the layout/paint/event hot paths (one indirect call per leaf
  per frame — negligible vs rasterization) and a touch more boilerplate. *Accept:*
  an external crate defines a leaf widget (custom layout/paint/event/semantics)
  and the agent drives it unmodified — the T7.2 acceptance, but real.
- **E3. `build_node` consumes the `Element` tree. ✅ done.** Flipped
  `build_node(&Element)` → `build_node(Element)`; each node's fields now **move**
  into `NodeMeta` (the 256 B `LayoutStyle`, the `image` pixel `Vec`, strings/
  `Vec`s; `Rc` handlers without a refcount bump). Goldens unchanged.

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

E (core type design: E3 build_node move fix · E1 leaf-content enum · E2 Widget trait) — parallel; E2 gates third-party widgets
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
