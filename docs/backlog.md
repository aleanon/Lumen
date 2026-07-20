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
- **A1** — `Renderer` trait + `TinySkia` **and** `Wgpu` backends (the latter
  behind the default-on `wgpu` feature), runtime-selectable via
  `renderer_override`.
- **Rendering/perf plan (R0–R3)** — golden diff harness; full GPU command
  coverage on `Wgpu` (rounded/bordered rects, `lyon` paths, gradients,
  layers/clip/opacity, `backdrop-filter`, images, HiDPI, display-list order);
  retained display list + damage/incremental repaint; GPU glyph atlas
  (`DrawCmd::GlyphRun` on both backends, physical-size HiDPI text). R4 (threaded
  layout) parked — see below.
- **B2/B3/C1/D1** — rich TextStyle; cached PNG assets; `.lss` hot reload; spring motion.
- **E1/E2** — Element→NodeContent enum; LeafWidget trait (first-class custom leaves).
- **Gallery redesign** — every iced-parity example now matches the stopwatch hero
  style (the plain ones — gradient/events/styling/websocket/pane_grid — elevated;
  the rest already conformed).
- Prior: live-window agent endpoint; design-analysis APCA contrast; resize fix;
  paint caches (text/shadow) + shadow-ring blit + hover.

## ⏸ Sandbox-blocked (need hardware / OS / external infra to *verify*)

- **A4 — Desktop OS integration** (native menus, clipboard, file/color dialogs,
  OS drag-and-drop, tray, notifications, multi-window/monitor). *Re-graded
  2026-07 (plan P.3): NOT sandbox-blocked* — this dev box has a live X11
  session, so the wiring is landing and live-verified here: arboard clipboard
  (P.3a ✅), rfd file-open dialogs (P.3b ✅), muda menus + accelerators
  (P.3c ✅; Linux/winit has no menubar attachment point — accelerators +
  `menu.invoke` are the activation paths there), OS drag-and-drop + desktop
  notifications + system tray (P.3e ✅ — tray menu hosts the app MenuModel;
  live-verified: SNI registration + a dbusmenu click driving app state).
  Multi-window (P.3d ✅): declared windows realized as real OS windows over
  the shared reactive store (live-verified pixel propagation). Remaining:
  per-window agent verbs.
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

- **R4 — Multi-threaded layout** (rendering/perf plan; **parked 2026-06-28**). Lay
  out very large *non-virtualized* trees on scoped threads. *Why parked, not
  blocked:* feasible with no new deps and **no taffy fork** — `TaffyTree` is an
  instantiable struct, so independent subtrees can each `compute_layout` on their
  own `std::thread::scope` thread (the `scene.rs` culling model) and be stitched
  by offset. The catch is layout's parent↔child size dependency: you can't
  parallelize *inside* one `TaffyTree` (one `&mut self`, shared measure cache), so
  it needs a real two-phase driver — measure independent regions → solve the upper
  tree → solve each region in parallel within its fixed box → offset — plus a
  1-vs-N byte-identical determinism test, gated behind a node-count threshold.
  *Why low priority:* the usual scalability answer is **virtualization** (already
  done for lists/`vlist`); a 10k-node non-virtualized tree is the narrow target.
  *First step:* R4.1 — identify the fork seam (flex/grid children whose size is
  fixed once the parent's available space is known) and prototype the split behind
  a threshold. (Options ruled out: parallelizing inside one tree would need a
  taffy fork/upstream; replacing taffy contradicts ADR-004.)
- **Incremental paint / retained per-subtree display lists** (perf; identified
  2026-07-01). On a frame that *does* rebuild, ~928 µs (gallery) goes to glyph-run
  + display-list emission — the dominant remaining per-frame cost after the
  shaped-text cache and skip-when-unchanged landed. *Why deferred, not urgent:* a
  changed frame is already well within the 16 ms budget, so this is headroom.
  **Now planned as `plan-rendering-performance.md` Phase R5** (R5.1 fragmented
  emission → R5.2 reuse clean fragments → R5.3 translate-reuse), tractable because
  the F0–F4 fine-grained work built the dirty-subtree structure it needs. *Queued
  after the F5 authoring sugar.*
- **Authoring sugar** (`For` keyed lists + list-GC, reactive `class`/`bind!`) —
  the F3 tail. **Now planned as `plan-fine-grained-view.md` Phase F5**; `text!` +
  the `Dynamic`/`Prop` primitive already ship. *Next up.*
- **B3 codecs** — jpeg/webp/avif decode (new deps → ADR); PNG ships now.
- **D1 motion** — gesture-driven interruptible animations + shared-element
  transitions on top of the `motion::spring` primitive.
- **GPU-only lean build** (`cpu` feature, default-on) — *deferred 2026-06-30,
  needs a product decision.* Let a known-GPU target `--no-default-features
  --features wgpu` and strip the CPU rasterizer for a smaller binary. *Why
  deferred, not trivial:* `tiny-skia` is also the framework's PNG codec
  (`RgbaImage::to_png/from_png`, image assets, GPU-screenshot encode), so a full
  drop needs PNG re-routed to the `png` crate; and `App::new`/`DefaultRenderer`
  hard-bake `TinySkia` as the *infallible* default renderer, so the ergonomic
  constructor must become conditional and a GPU-only build loses the GPU-less
  fallback + headless render + CPU golden path. Bounded blast radius (workspace
  tests/examples keep `cpu` on via feature unification). *First step:* gate the
  `cpu` module + `TinySkia` + the `WgpuFallbackTinySkia` fallback arm behind a
  default-on `cpu` feature; make `App::new`'s default-renderer constructor
  `#[cfg(feature="cpu")]`; measure the binary-size delta. (Full plan lives in the
  Track-1c plan doc, step 1c.8.)

### ✅ Done — in-sandbox, no new deps / no ADR (2026-06-28)

1. **`.lss`/`Element` borders.** `Element::border` + `.lss` `border`/
   `border-width`/`border-color` → the existing `DrawCmd::Rect { border }`
   primitive; precedence `.lss → element → focus ring`; outline-only boxes paint.
   *Remaining (deferred):* per-side / per-corner borders + inset (border-box)
   stroke — needs a `Border` primitive change.
2. **Glass refraction (Liquid Glass).** `BackdropFilter` gains opt-in
   `refraction` (rounded-edge SDF lensing) + `specular` (rim highlight) on both
   backends (CPU golden contract + GPU shader), wired through `.lss`
   `backdrop-filter: … refraction(px) specular(n)` and shown in `examples/glass`.
   *Note:* the look is a principled first cut; the falloff/strength constants may
   want a tuning pass.
3. **Additive font registration.** `TextEngine::register_font(bytes)` +
   `TextStyle::family` / `Label::family` + `App::with_font`; unknown families fall
   back to the bundled default. No system enumeration (ADR-005 intact).
   *Remaining:* `.lss font-family` cascade and a second-font visual golden (needs
   a small bundled test font); system-font enumeration stays in B1/sandbox-blocked.

## Notes

- New runtime dependencies (`lyon`, `rfd`, `muda`, `arboard`, `accesskit_winit`,
  codecs) are all **ADR-003 escalations** — each needs a decision-log entry
  before adding.
- "Sandbox-blocked" means *acceptance can't be verified here*; several have a
  testable slice (clipboard via arboard, AccessKit-tree diff, software decode,
  WASM CPU goldens) that can land first with the OS/AT/device half deferred.
