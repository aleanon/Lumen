# Lumen — Cross-Platform Native Readiness: what remains

*Determination as of 2026-06-22. Verified against the code, not the milestone
labels. Companion to `app-framework-readiness.md` (desktop-focused, 2026-06-18)
and `backlog.md`.*

## Verdict

Lumen is **headless-complete and architecturally sound, but not yet shippable as
a cross-platform native app framework.** A developer can build, test, and
agent-drive a UI today; they cannot yet ship a real app to end users on all four
targets. The gaps are concentrated in three places, none of them the core:

1. **The data layer** — *mostly built (2026-06-22):* `cx.resource`/`cx.task` +
   a thread-pool executor feed background results into state without freezing the
   UI thread. What remains is a bundled async runtime + HTTP client and a wasm
   executor (Part D, ADR-003).
2. **The render path** — every shell CPU-rasterizes the whole window and blits it
   as one texture per frame; the GPU display-list backend exists but is offscreen
   and covers only rects+images. There is no true GPU rendering in any live shell.
3. **The OS bridges & platform shells** — the portable APIs (system, semantics,
   bundle) exist, but desktop OS integration is unwired, only Android has a real
   loop, and iOS/web are one-shot render cores assembled by hand.

Everything below is graded **Blocker** (can't ship a usable app without it),
**Serious** (ships but is not credibly "native"), or **Polish**.

## What is genuinely solid (do not redo)

Core (tree/signals/state/events/semantics), layout (Taffy flex/grid/absolute +
RTL), styling (`.lss` parse/cascade/tokens/transitions + `backdrop-filter`),
text shaping (parley/swash: bidi/CJK/IME model/wrapping), the **deterministic
CPU reference renderer** (the golden contract), the `LeafWidget` trait + `Frame`
2D API (custom widgets/charts), the test+agent tooling, and the **portable**
system/semantics/bundle APIs. The reactive runtime is a pure function of (state,
queued events, clock) and already exposes `inject(Event)` — the seam async
results will arrive through.

## Blockers — cannot ship a real app

### 1. Async / data layer  *(was the single biggest gap — now largely built)*
- **DONE (2026-06-22):** `lumen_core::tasks` + `cx.resource`/`cx.resource_blocking`
  /`cx.task`/`cx.task_blocking` ship. Background work runs on a `Spawner`
  (`Headless<R, E>`, defaulted generic) — `InlineSpawner` (deterministic default),
  `ManualSpawner` (tests), `ThreadPoolSpawner` (live shell). Results feed back via
  a deferred-op channel drained on the UI thread (pump stays pure). `Resource<T,E>`
  is data+flags with stale-while-revalidate; refetch is dep-hash-tracked with
  generation-guarded cancellation. The shell wires a waker so results schedule a
  frame. `examples/data` demonstrates it. **No new dependencies.**
- **REMAINS (Part D, ADR-003):** an async *runtime* + HTTP/WS client (so fetchers
  do real non-blocking network I/O — evaluate blocking `ureq` first since the
  thread pool already gives concurrency), and a `WasmSpawner` for browser parity.
  Today a fetcher can already do blocking I/O on the thread pool; what's missing
  is the bundled transport + the wasm executor. The `websocket` example still uses
  a blocking `tungstenite` round-trip pending this.

### 2. No true GPU rendering — full-frame CPU raster every frame
- **Evidence:** `Presenter::present` does `create_texture` + `queue.write_texture`
  of `h.screenshot()` **each frame** (desktop); Android software-blits; iOS/web
  blit CPU bytes. `gpu.rs` is offscreen-only and its `match` has `_ => {}` for
  "gradients/paths/layers/glyphs/shader: GPU later" — it handles only Rect+Image.
- **What remains:** a real GPU surface backend behind the existing `Renderer`
  trait — quad pipeline (have), path/stroke tessellation (`lyon`, ADR), gradients,
  glyph atlas, layer clip/opacity/blend, `BackdropFilter`. Until then the perf
  ceiling is "rasterize the entire window on the CPU at 60fps," which won't hold
  for large/HiDPI windows, animation, or low-end mobile.

### 3. Desktop OS integration unwired (A4)
- **Evidence:** `lumen-shell` deps are only `winit`/`wgpu`/`notify`; no
  clipboard/menu/dialog refs. Portable `system::{MenuModel,WindowDesc,
  SystemRequest}` + `Headless::clipboard_*` exist but nothing bridges them to the
  OS.
- **What remains (ADR-003 deps):** real clipboard (`arboard`), native menu bar +
  context menus (`muda`), file/color dialogs (`rfd`), OS drag-and-drop,
  notifications, tray, and **multi-window/multi-monitor** (the shell early-returns
  if a window exists). An app without copy/paste, a menu bar, or a file-open
  dialog isn't shippable for most use cases.

### 4. Mobile/web shells are not turnkey
- **Evidence:** `lumen-shell-ios` and `-web` are single `render_into()` one-shot
  functions (render one frame into a buffer); the event loop, input, IME, and
  Metal/canvas presentation live in hand-assembled platform templates (Obj-C
  built **only on macOS**; JS). `lumen-shell-android` is the only real loop
  (149-line native-activity software blit + touch).
- **What remains:** a continuous-frame runtime per platform (input → pump →
  present, animation/idle scheduling, lifecycle/safe-area), so "cross-platform"
  is real rather than "desktop + Android, plus iOS/web you assemble yourself."

## Serious — ships, but not credibly "native"

### 5. Fonts: one bundled font, one weight
- **Evidence:** `lumen-text` bundles a single `GoNotoKurrent-Regular.ttf`,
  `system_fonts: false`; bold is **synthesized** (faux-embolden); no italic faces,
  no custom/system font registration, `TextStyle` family is fixed.
- **What remains:** a `register_font(bytes)` + family/fallback API (safe,
  in-sandbox, keeps the deterministic default); optional system-font backend
  (breaks goldens → ADR-005). Real typography (true bold/italic, brand fonts,
  color emoji) is table stakes for shippable apps.

### 6. Accessibility bridge not live (A5) — *closer than it looks*
- **Evidence:** `lumen_widgets::a11y` **already** builds an `accesskit::TreeUpdate`
  from the semantics tree (`role_to_accesskit` exhaustive map + `build_tree`).
  What's missing is the `accesskit_winit` platform adapter in the shell and
  real-AT verification.
- **What remains:** wire the adapter (publish the tree, route a11y actions back
  through `inject()`); the tree-equality test is doable in-sandbox, the
  live-screen-reader smoke test needs real AT.

### 7. Distribution: bundle, not installers (C3)
- **Evidence:** `lumen-cli/src/dist.rs` emits a portable bundle + `manifest.json`;
  signing/notarization/auto-update and msix/dmg/AppImage/ipa are explicitly
  deferred (the Android `.apk` script exists).
- **What remains:** per-OS signed/notarized installers + delta auto-update —
  needs real toolchains/certs (gate behind CI secrets).

### 8. Asset codecs: PNG only
- **Evidence:** PNG decode/cache ships; jpeg/webp/avif and video/audio are
  deferred (new deps → ADR). SVG renders a small subset (rect/circle/path).
- **What remains:** the common image codecs + a media path for the shell.

## Polish — quality and parity

- GPU display-list completeness (paths/gradients/glyphs/layers/backdrop/HiDPI).
- Motion: gesture-driven interruptible animations + shared-element transitions on
  top of `motion::spring`.
- Perf at scale: **layout is single-threaded** (only render culling uses scoped
  threads, `scene.rs`); GPU damage/partial redraw; the `01 §9` budgets on real
  runners.
- Glass refraction/lensing (Liquid Glass) + glass rim border (borders aren't
  plumbed from `.lss`/`Element` yet).

## Per-platform readiness (today)

| Target | Runtime loop | Render | Input | OS integration | Verdict |
|---|---|---|---|---|---|
| **Desktop** (winit/wgpu) | ✅ real | CPU raster → texture blit | ✅ kbd/mods/IME/DPI | ❌ none wired | **closest** — needs A1+A4 |
| **Android** | ✅ native-activity loop | CPU software blit | ◑ touch | ❌ | usable demo, not native-grade |
| **iOS** | ❌ render core + Obj-C template (macOS-only) | CPU bytes → Metal | template | ❌ | **not turnkey** |
| **Web/WASM** | ❌ render core + JS template | CPU bytes → canvas | template | n/a | **not turnkey**; WebGPU path unbuilt |

## Recommended sequence (highest leverage first)

1. ~~**Async/data layer**~~ **(Blocker #1 — DONE 2026-06-22)** `cx.resource`/`cx.task`
   + Inline/Manual/ThreadPool executors ship. Remaining: a bundled async runtime +
   HTTP client and a `WasmSpawner` (Part D, ADR-003).
2. **GPU surface backend** (Blocker #2) — *now the top remaining blocker.* The
   perf/quality unlock across *all* platforms; one backend behind the existing
   `Renderer` trait (now a defaulted generic) benefits desktop+mobile+web.
3. **Desktop OS integration** (Blocker #3) — clipboard first (most testable),
   then menus/dialogs/DnD/multi-window. Makes desktop genuinely shippable.
4. **Turnkey mobile/web loops** (Blocker #4) — promote the `render_into` cores to
   real continuous runtimes; reuse the desktop input/scheduling model.
5. **Fonts (5)** and **AccessKit adapter (6)** — both have safe in-sandbox slices.
6. **Installers (7)**, **codecs (8)**, then **polish**.

Items 3/6/7 and parts of 4/8 are **sandbox-blocked for *verification*** (need a
real desktop session, devices, certs, screen readers) — but most have a testable
slice that can land now with the OS/device half deferred (see `backlog.md`).
