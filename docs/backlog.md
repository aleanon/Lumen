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
- **A1** — `Renderer` trait + `CpuRenderer` **and** `GpuRenderer` backends, runtime-
  selectable. (GPU path/gradient/glyph tessellation follow-on.)
- **B2/B3/C1/D1** — rich TextStyle; cached PNG assets; `.lss` hot reload; spring motion.
- **E1/E2** — Element→NodeContent enum; LeafWidget trait (first-class custom leaves).
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
- **Async runtime + HTTP/WS client + `WasmSpawner`** (the data layer's Part D).
  *Done already (no deps):* the executor/data layer — `Spawner` + `Sink` +
  `cx.resource`/`cx.task` + `InlineSpawner`/`ManualSpawner`/`ThreadPoolSpawner`,
  shell waker (`lumen_core::tasks`, `examples/data`). *Why blocked:* a bundled
  async runtime + HTTP/WS client (`tokio`+`reqwest`, or blocking `ureq`) and
  `wasm_bindgen_futures` are ADR-003 escalations. *First step:* evaluate `ureq`
  (blocking, tiny — the thread pool already provides concurrency) for a `lumen-net`
  convenience crate; `WasmSpawner` = `spawn_local` for browser parity. Until then a
  fetcher can already do blocking I/O on the thread pool; only the bundled
  transport + wasm executor are missing.

## 🔭 Follow-on within completed items (smaller, additive)

The scope-deferred items are all implemented (see Done). These are the remaining
*extensions* inside them, each additive and behind the now-shipped abstractions:

- **A1 GPU** — path/stroke tessellation (`lyon` — ADR), gradients, glyph atlas,
  layer clip/opacity, and HiDPI scaling on the GPU backend (renders 1:1 today).
- **B3 codecs** — jpeg/webp/avif decode (new deps → ADR); PNG ships now.
- **D1 motion** — gesture-driven interruptible animations + shared-element
  transitions on top of the `motion::spring` primitive.
- **B1 fonts** — custom/system font registration + `TextStyle` family (ADR-005
  determinism considerations; see sandbox-blocked note).
- **Glass refraction (Liquid Glass)** — the `backdrop-filter: blur()/saturate()`
  primitive ships (CPU renderer + `.lss` cascade + `examples/glass`). The
  remaining Apple "Liquid Glass" look is *refraction/lensing*: bending the
  blurred backdrop with a per-pixel displacement (a rounded-rect normal/height
  map) plus a moving specular highlight. *First step:* a `DrawCmd` displacement
  variant (or extend `BackdropFilter` with an optional displacement map) sampled
  deterministically on the CPU backend, with the GPU path as a shader. Needs the
  edge-normal model worked out; larger than the blur slice, hence deferred.
  Also pending: GPU-backend support for `BackdropFilter` (CPU is the contract;
  the GPU backend only handles Rect/Image today) and a hairline rim border for
  the glass edge (borders aren't plumbed from `.lss`/`Element` yet).

## Notes

- New runtime dependencies (`lyon`, `rfd`, `muda`, `arboard`, `accesskit_winit`,
  codecs) are all **ADR-003 escalations** — each needs a decision-log entry
  before adding.
- "Sandbox-blocked" means *acceptance can't be verified here*; several have a
  testable slice (clipboard via arboard, AccessKit-tree diff, software decode,
  WASM CPU goldens) that can land first with the OS/AT/device half deferred.
