# Backlog — app-framework readiness items not completed in-sandbox

Companion to `docs/app-framework-readiness.md`. Every plan item is resolved here
to a definite state: **done**, **sandbox-blocked** (can't be built/verified in
this environment), or **scope-deferred** (doable, but a large multi-session
change that needs review / an ADR). Each blocked/deferred item lists *why* and a
*first step*.

## ✅ Done (this initiative)

- **A2** desktop input — real modifiers + IME wired in the shell.
- **A3** HiDPI — logical-px runtime, physical-px raster (`render_scaled`,
  `Headless::scale`), shell scale handling + logical pointer coords.
- **C2 (top level)** — `rebuild()` contains build/layout/paint panics; the window
  keeps its last frame and reports `E0701` (subtree `error_boundary` already
  existed).
- **E3** — `build_node` consumes the `Element` (per-node clones → moves).
- **A1 (abstraction)** — `Renderer` trait + `CpuRenderer`; `Headless` is generic
  over a `Box<dyn Renderer>` (`set_renderer`/`renderer_name`). GPU backend remains.
- **Gallery redesign** — every iced-parity example now matches the stopwatch hero
  style (the plain ones — gradient/events/styling/websocket/pane_grid — elevated;
  the rest already conformed).
- Prior: live-window agent endpoint; design-analysis APCA contrast; resize fix;
  paint caches (text/shadow) + shadow-ring blit + hover.

## ⏸ Sandbox-blocked (need hardware / OS / external infra to *verify*)

- **A4 — Desktop OS integration** (native menus, clipboard, file/color dialogs,
  OS drag-and-drop, tray, notifications, multi-window/monitor). *Why blocked:*
  needs a real desktop session and new platform deps (e.g. `rfd`, `muda`,
  `arboard`) that are ADR-003 additions; native dialogs/menus can't be
  acceptance-tested headlessly. The portable APIs (`lumen-widgets::system`)
  already exist and are agent-synthesizable — only the OS *wiring* is missing.
  *First step:* ADR for the platform deps; wire `arboard` clipboard (most
  testable) behind the existing `SystemRequest`/`Headless::clipboard_*` API.
- **A5 — AccessKit platform bridge.** *Why blocked:* the adapter (`accesskit_winit`,
  a new dep) can be wired, but the acceptance is "Orca/NVDA/VoiceOver read the
  live UI" — real screen readers aren't available here. *First step:* ADR + wire
  `accesskit_winit` feeding `Headless::semantics_doc()`; verify the AccessKit
  tree matches headless `semantics_json` (that part *is* testable) and defer the
  live-AT smoke test.
- **C3 — Distribution & packaging** (signed/notarized installers, auto-update).
  *Why blocked:* needs per-OS toolchains, code-signing certs, and notarization
  services. *First step:* keep `lumen package` producing unsigned bundles; gate
  signing behind CI secrets on real runners.
- **B1 — System font loading.** *Why partly blocked:* enabling fontique's system
  backend is platform-specific *and* breaks determinism/goldens (ADR-005 chose a
  single bundled font on purpose). *First step (safe, in-sandbox):* an additive
  `register_font(bytes)` + family-selection API that keeps the bundled font as
  the deterministic default (no system enumeration) — this part is doable; the
  system-enumeration half stays blocked/needs an ADR.
- **D2 — Audio/video/capture**, **D4 — web + mobile shell parity**, parts of
  **D3** (perf gates on reference runners). *Why blocked:* codecs/hardware decode
  + new deps; browser/iOS CI and devices; the Android emulator exists locally but
  is heavy. *First step:* the deterministic software-decode CI path (D2) and the
  WASM/CPU golden path (D4) are the testable slices to start with.

## 🏗 Scope-deferred (doable here, but large / needs review or an ADR)

- **A1 — GPU surface backend** *(abstraction done; this is the remaining half).*
  The `Renderer` trait + CPU backend ship. Build the GPU surface path behind the
  same trait: rect (have); **paths via `lyon`** — new dep/ADR; gradients; glyph
  atlas; layers. *Why deferred:* multi-day effort + a new dependency. *First
  step:* a `GpuRenderer: Renderer` rendering rects/images to a texture (extend
  the existing offscreen `gpu.rs`), parity-tested vs CPU on lavapipe; add paths
  next.
- **B2 — Rich `TextStyle`** *(blast radius confirmed: ~17 `TextStyle { … }`
  literal sites across crates/tests/examples).* Add `line_height`/`letter_spacing`
  (default no-op) + parley wiring; update every literal. Deferred as a focused
  pass — mechanical but wide, and low urgency now the gallery looks right.
- **E1 — Slim `Element` / leaf-content enum.** *Why deferred:* changes the public
  `Element` field surface (`text`/`image`/`canvas` → `content`), touching every
  widget constructor, `theme`, and all examples — a broad breaking refactor best
  reviewed as one PR. Goldens are the safety net. *First step:* introduce
  `NodeContent` internally, migrate constructors, then flip the public fields.
- **E2 — Implement the spec's `Widget` trait (`02 §3`).** Major architecture
  (composites = functions; leaves = `dyn LeafWidget` lowered to
  `NodeContent::Custom`); unlocks first-class third-party widgets (T7.2). Depends
  on E1. Memoization rides `PartialEq`/hash on the trait (see readiness §E).
- **C1 — Desktop hot reload.** Wire the dev-server file-watcher (the `notify`
  dep is whitelisted) into the running shell via the `EventLoopProxy` bridge:
  on `.lss` change call `Headless::set_stylesheet`. *Why deferred:* needs the
  dev-server wire protocol fleshed out; moderate. *First step:* a `--watch <lss>`
  flag on the `win` example that reloads one stylesheet (tier-1) over the bridge.
- **B3 — Assets.** Image codecs behind a shared cache; declarative asset refs.
  New deps (codecs) → ADR. *First step:* PNG/JPEG decode behind the existing
  `RgbaImage` path.
- **D1 — Motion system** (springs, interruptible gestures, shared-element
  transitions). Large; the virtual-clock + `animate()` substrate is in place.

## Notes

- New runtime dependencies (`lyon`, `rfd`, `muda`, `arboard`, `accesskit_winit`,
  codecs) are all **ADR-003 escalations** — each needs a decision-log entry
  before adding.
- "Sandbox-blocked" means *acceptance can't be verified here*; several have a
  testable slice (clipboard via arboard, AccessKit-tree diff, software decode,
  WASM CPU goldens) that can land first with the OS/AT/device half deferred.
