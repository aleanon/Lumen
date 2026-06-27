# Plan: rendering & performance (GPU surface, damage, glyph atlas, threaded layout)

*Design + build plan, 2026-06-23. Companion to
`plan-executor-and-renderer-generics.md` (the renderer **seam** — `App<R =
DefaultRenderer>`) and to `cross-platform-readiness.md` (Blocker #2 + the "Perf
at scale" polish line). This plan is the work that runs **behind** that seam.*

> **Renderer naming (2026-06-26).** The backends are `TinySkia` (CPU reference,
> the golden contract and `DefaultRenderer`) and `Wgpu` (GPU), with
> `WgpuFallbackTinySkia` for "GPU when available, else CPU". `Wgpu`/`lyon` sit
> behind a default-on `wgpu` cargo feature. Backend choice at runtime goes
> through `renderer_override` (`--wgpu`/`--tiny-skia`/`LUMEN_RENDERER`). (Earlier
> revisions of this plan called these `CpuRenderer`/`GpuRenderer`.)

> **Status (2026-06-24).** R0 ✅ done. **R1 ✅ COMPLETE** — the GPU backend
> matches the CPU reference for every command the framework produces (rects incl.
> rounded/border, paths, gradients incl. rounded, layers/clip/opacity, images
> nearest+bilinear, glass `backdrop-filter`, text-as-image), draws in display-list
> order, honors HiDPI, and drives the live window (R1.1). Scoped out (no
> producer): non-source-over blends and GPU `DrawCmd::Shader` (`ShaderWidget`
> pre-rasterizes to an image). Deferred (perf): zero-copy render-to-surface (the
> live-window agent needs a per-frame readback). **R2 ✅ done. R3.1 ✅ done.**
> R3.2–R3.5 and R4 remain (R4 constrained by taffy — see its section).
>
> <details><summary>(historical detail)</summary>
>
> R1 offscreen backend ✅ done — `Wgpu` matches the CPU reference within
> tolerance for rects (R1.2), paths (R1.3, `lyon`+MSAA), gradients (R1.4, Oklab
> ramp), layers/clip/opacity (R1.5, render-to-texture; **linear-light blending**
> since 2026-06-25 — `Rgba8UnormSrgb`, CPU reference stays gamma so they match on
> opaque/non-AA content and intentionally differ on blended/AA pixels),
> and HiDPI (R1.6), gated at 1× and 2×. **R1.1 ✅ done** — the desktop shell now
> rasterizes the live window through the GPU backend (dynamic-renderer seam,
> `Box<dyn Renderer>`, GPU-if-available else CPU), keeping the `Rgba8Unorm`
> linear-light target; verified by a headless boxed-GPU sanity test and a live launch
> ("GPU renderer active" + a valid agent screenshot). Remaining in R1:
> non-source-over blends, rounded gradient rects, `BackdropFilter` on GPU, GPU
> `DrawCmd::Shader`, strict intra-layer draw order, and (perf) zero-copy
> render-to-surface — today the GPU frame is read back for the always-on agent
> and blitted by the presenter. **R3.1 ✅ done** — per-glyph CPU
> raster cache in `lumen-text` (byte-identical; a changed string only
> rasterizes new glyphs). **R2 ✅ done (paint side)** — retained display list +
> `damage_between` diff + `Renderer::render_damage` + incremental composite into
> the retained frame; idle pumps repaint nothing; `FrameStats.damage` drives the
> shell idle-skip. *Scope note:* R2.1/R2.2 (dirty-flag marking + incremental
> `relayout_subtree`) are **not applicable** to the current full-rebuild reactive
> model — the tree+layout recompute each pump, so damage is derived by diffing
> display lists, not by propagating node dirty flags (the flags remain for a
> future incremental-tree model). Shell partial-*tile* upload (vs idle-skip +
> full upload) needs a Presenter texture-retention refactor — deferred (minor;
> the dominant CPU raster is already saved). R3.2–R3.5 and R4 pending (R4
> constrained by taffy — see its section).
>
> </details>
>
> **Scope.** Turns the five priority recommendations into committable phases:
> (1) a real GPU surface backend, (2) activating the dormant dirty-flag /
> incremental-layout / damage path, (3) a glyph atlas, (4) multi-threaded layout,
> (5) keeping the CPU renderer as the golden reference. Phases are ordered by the
> recommendation's own priority; dependency edges are called out where the order
> can't be strictly linear.

## Foundational invariant (do not violate)

The **CPU renderer stays the golden/deterministic reference** and the
headless/test/agent path. Every phase here is measured against it: the GPU
backend must match CPU goldens within a documented tolerance, and the
damage/incremental path must produce frames **byte-identical** to a full repaint.
This is recommendation #5 promoted from "asset" to an enforced guardrail — see
**Phase R0**. It mirrors the determinism contract in
`plan-executor-and-renderer-generics.md`: `pump()` is a pure function of *(state,
queued events, clock)*; incremental rendering is an optimization that must not
observably change output.

## Where each recommendation lands

| Rec | Phase | One-line |
|---|---|---|
| 1. GPU surface backend | **R1** | Live `wgpu` surface behind `Renderer`; quad → paths (`lyon`) → gradients → layers/blend → HiDPI. |
| 2. Activate dirty flags | **R2** | `DIRTY_LAYOUT`/`DIRTY_PAINT` drive `relayout_subtree` + retained display list + `render_damage`; present changed tiles only. |
| 3. Glyph atlas | **R3** | Atlas + instanced glyph quads on the GPU; retires per-string sprite blits. (Sub-capability of R1; own phase because it's separately prioritized.) |
| 4. Multi-threaded layout | **R4** | Extend the `scene.rs` scoped-thread model to layout for large non-virtualized trees. |
| 5. Keep CPU renderer | **R0** | Differential-test harness: GPU≈CPU and damage==full-repaint, enforced in CI. |

---

# Phase R0 — Golden guardrail (do this first; it gates R1–R4)

**Why first:** every later phase needs a way to prove it didn't regress output.
Today the CPU↔GPU comparison is ad hoc and `render_damage` is exercised by a
single golden test. R0 turns "keep the CPU renderer as the reference" into a
reusable, enforced harness.

## Current state
- `TinySkia` is the default and the golden contract (`cpu_goldens.rs`).
- `render_damage` exists (`cpu.rs:61`) and is byte-identical to the cropped full
  frame *by construction*, but only one test calls it.
- GPU/CPU parity is checked by calling each `render` directly (per the seam plan's
  A5), not by a shared differential harness.

## Steps (each independently green)
- **R0.1.** Add `tests/diff_harness.rs` in `lumen-render`: `assert_frames_eq(a, b,
  tol)` (max per-channel delta + count of differing pixels), plus a corpus of
  display lists (one per `DrawCmd` class + the example screens' lists).
- **R0.2.** `cpu_vs_gpu` differential test: for each corpus DL, `TinySkia` vs
  `Wgpu` within a documented AA tolerance; **skip with a clear log when no
  GPU adapter** (CI runners without a device) — never silently pass.
- **R0.3.** `damage_equivalence` test: for a corpus of (DL, dirty-rect) pairs,
  assert `render_damage(dl, dirty)` is byte-identical to `render(dl)` cropped to
  `dirty`. This is the contract R2 must keep.
- **R0.4.** Wire both into `cargo test --workspace`; document the tolerance and the
  "GPU absent ⇒ skip" policy in the renderer crate docs.

*Acceptance:* the harness fails on an injected 1-pixel GPU divergence and on an
injected damage-vs-full mismatch; passes on `main`; skips cleanly with no GPU.

---

# Phase R1 — GPU surface backend behind `Renderer`  *(Rec #1 — top priority)*

The single biggest realizable win; unblocks HiDPI/animation/mobile/web. Built
incrementally so each `DrawCmd` class lands behind the existing trait with the CPU
renderer as the oracle (R0).

## Current state
- `Renderer` trait: `render_frame(&mut self, &DisplayList, w, h, scale, bg) ->
  RgbaImage`-ish + `name()`; impls for `TinySkia`, `Wgpu`, `Box<R>`.
- `DrawCmd` vocabulary already complete: `Rect, Path, Image, GlyphRun, PushLayer,
  PopLayer, Shader` with `Fill`/`Stroke` styles, gradients, `BlendMode`,
  `CornerRadii`, `BackdropFilter`.
- `gpu.rs` is **offscreen-only** and its `exec` match handles **only**
  `DrawCmd::Rect` and `DrawCmd::Image`; everything else is `_ => { /* GPU later */
  }`. No live surface — every shell CPU-rasterizes the whole window and blits
  `h.screenshot()` as one texture per frame (`lumen-shell` `present`).

## Target
A `Wgpu` that (a) renders the full `DrawCmd` set offscreen matching CPU
within tolerance, and (b) drives a **live `wgpu` surface** in the shell via
`App::new(build).with_renderer(Wgpu::new()?)` (the seam from Part A is
already in place). CPU stays the default and the fallback.

## Sub-phases (each independently green, each gated by R0)
- **R1.1 — Surface plumbing.** Add a `present_to_surface` path so the shell can
  hand `Wgpu` a `wgpu::Surface` (configured for the window) instead of
  rasterizing to bytes. Keep the offscreen `render_frame` for goldens/agent. Shell
  selects GPU-if-available else CPU at the entry branch (Part A4 left the hook).
- **R1.2 — Rect pipeline on-surface.** Promote the existing quad/rect+image
  handling to the surface path: instanced rounded-rect quads (corner radius + AA
  in the fragment shader), solid fills, image blits with the sampler/filter. This
  is parity with what `gpu.rs` already does, now live. *Milestone: `examples/hello`
  renders through the GPU surface.*
- **R1.3 — Path & stroke tessellation (`lyon`, ADR-003).** Tessellate
  `DrawCmd::Path` (`Fill`/`Stroke`) with `lyon` into triangles; cache tessellations
  keyed by path+style hash (paths are static across most frames). New dep ⇒
  decision-log entry. *Milestone: SVG/canvas example screens render on GPU.*
- **R1.4 — Gradients.** Linear/radial/conic as fragment-shader brushes (or a small
  ramp texture for conic); match CPU's `fill_conic`/spread modes. *Milestone:
  palette/progress example screens match CPU within tolerance.*
- **R1.5 — Layers: clip / opacity / blend.** Implement `PushLayer`/`PopLayer` as
  render-to-texture (or scissor for axis-aligned rect clips, the common case) with
  group opacity and `BlendMode` compositing; then `BackdropFilter` (sample the
  layer below, blur+saturate) reusing the existing blur primitive. *Milestone: the
  scrollable clip + modal/toast overlays + backdrop screens match CPU.*
- **R1.6 — HiDPI / resize.** Honor `scale` (logical→physical) end-to-end;
  reconfigure the surface on resize without a full rebuild; verify crisp text/edges
  at 2× (depends on R3 for text).

*Acceptance:* every example screen renders through the GPU surface; the R0
`cpu_vs_gpu` differential passes for all corpus DLs within tolerance; CPU remains
the default and a working fallback; `examples/*` open via `just run <ex>` on the
GPU path on this box (RTX 4070 / lavapipe).

> **Note (Rec #1 ⊇ Rec #3).** The recommendation folds "glyph atlas" inside the
> GPU backend. Text is large enough to warrant its own phase (**R3**) but is a
> hard dependency of R1.6 (crisp HiDPI text) and of any text-heavy GPU screen.

---

# Phase R2 — Activate the dormant dirty flags  *(Rec #2)*

Turn the framework's fine-grained reactivity into fine-grained *rendering*. The
pieces exist but are inert; this phase wires them end-to-end behind R0's
`damage_equivalence` guarantee.

## Current state
- `NodeFlags::DIRTY_LAYOUT` / `DIRTY_PAINT` exist (`tree.rs`) but nothing sets or
  reads them — `pump()` does a full `rebuild()` + full relayout + full paint every
  turn (`app.rs:310`).
- `lumen-layout::relayout_subtree` exists (`tree.rs:80`) but is unused by the
  runtime (only a layout fixture calls it).
- `render_damage` exists (`cpu.rs`) but the shell always presents the **whole**
  `h.screenshot()`.

## Steps (each independently green)
- **R2.1 — Mark dirt.** When a signal changes / the tree diffs during rebuild, set
  `DIRTY_LAYOUT` (geometry-affecting) or `DIRTY_PAINT` (paint-only, e.g. color) on
  the affected subtree roots. Add a debug assertion: anything reachable-changed is
  marked.
- **R2.2 — Incremental layout.** When only `DIRTY_LAYOUT` subtrees changed and
  their size constraints are unchanged at the boundary, call `relayout_subtree`
  instead of a full layout; fall back to full layout when a change crosses the
  subtree boundary. Validate against full-layout output on a fixture corpus.
- **R2.3 — Retained display list + damage regions.** Keep last frame's display list
  + per-node command spans; on rebuild, diff to compute a **damage rect** (union of
  changed nodes' old+new bounds). Empty damage ⇒ skip paint entirely.
- **R2.4 — Present only changed tiles.** Shell calls `render_damage(dl, damage)` and
  uploads only the damaged region(s) (`write_texture` sub-rect on CPU path;
  scissor + partial draw on the GPU path from R1). Idle frames present nothing.
- **R2.5 — Determinism gate.** Reuse R0.3: assert the damaged frame is
  byte-identical to a full repaint across the example corpus and a fuzz of random
  single-signal edits. Add a counter (nodes relaid-out / pixels repainted) to
  prove incrementality, exposed to the agent for tests.

*Acceptance:* editing one signal in `widget_gallery` relays out only its subtree
and repaints only its damage rect (asserted via the counters), and the resulting
frame is byte-identical to a full repaint; an idle pump presents nothing.

> **Dependency:** R2.4's GPU half needs R1.5 (scissor/layers). The CPU half
> (`write_texture` sub-rect) can land independently right after R2.3.

---

# Phase R3 — Glyph atlas  *(Rec #3)*

Replace per-string CPU raster blits with a GPU glyph atlas + instanced glyph
quads — essential for text-heavy and animated UIs.

## Current state
- `lumen-text` shapes via parley/swash and yields positioned `GlyphRun`s
  (`lib.rs:296`).
- The widgets paint layer **rasterizes whole strings into sprites**, caches them
  (`text_cache`, keyed by string+size+weight+color), and emits them as
  `DrawCmd::Image` blits. The `DrawCmd::GlyphRun` arm is a **stub** in both
  renderers (`cpu.rs:143`). So: no atlas; cache granularity is the whole string;
  animated/changing text thrashes the cache.

## Steps (each independently green)
- **R3.1 — Per-glyph raster cache.** Rasterize and cache individual *glyphs* (keyed
  by font+id+subpixel-position bucket+size), not whole strings — a CPU-side change
  that already cuts re-raster on text edits and is verifiable headless.
- **R3.2 — Atlas allocator.** Pack glyph bitmaps into a GPU texture atlas (shelf or
  skyline packer) with eviction; grow/rotate atlas pages on overflow.
- **R3.3 — Instanced glyph quads.** Implement the real `DrawCmd::GlyphRun` arm in
  `Wgpu`: one instanced draw per run sampling atlas UVs; alpha/coverage
  blend with the text color. CPU renderer implements `GlyphRun` too (currently
  stubbed) so goldens cover it.
- **R3.4 — Retire string sprites.** Switch the paint layer to emit `GlyphRun`
  instead of pre-rasterized `Image` sprites; keep the sprite path only as the CPU
  fallback if needed. Verify text screens against pre-R3 goldens within tolerance.
- **R3.5 — HiDPI subpixel.** Atlas entries keyed by physical size + subpixel bucket
  so 2× text is crisp (closes R1.6's text dependency).

*Acceptance:* a text-heavy screen renders via `GlyphRun` on the GPU; per-glyph
caching means a 1-character edit re-rasterizes ≤1 glyph (counter-asserted); CPU
`GlyphRun` output matches the old sprite path within tolerance.

---

# Phase R4 — Multi-threaded layout  *(Rec #4)*

The SoA layout design already invites it, and the threading pattern already exists
for culling. Extend it to layout for very large non-virtualized trees.

## Current state
- `scene.rs` culling splits work across **`std::thread::scope`** chunks sized by
  `available_parallelism()` above a threshold — **no rayon**, std-only
  (ADR-003-friendly). This is the model to copy.
- Layout (`lumen-layout`) is **single-threaded**.

> **Architectural finding (2026-06-24).** Layout is delegated to **taffy**
> (`TaffyTree::compute_layout`, ADR-004), which solves the whole tree serially in
> one call and is *not* externally parallelizable across disjoint subtrees of a
> single tree (shared internal measure cache; one `&mut TaffyTree`). R4's premise
> — "the SoA design already invites it" — actually refers to the **culling** pass
> in `scene.rs` (which is threaded), not to layout. So R4 as written cannot be a
> small drop-in: parallelizing the layout *solve* requires either (a) splitting
> independent regions into separate `TaffyTree`s computed on scoped threads and
> stitched by offset (a real change to how `lumen-widgets` drives layout), or
> (b) contributing parallelism upstream to taffy, or (c) replacing taffy
> (contradicts ADR-004). **Recommendation:** pursue (a) only for genuinely large
> non-virtualized trees, gated behind a node-count threshold and a 1-vs-N
> byte-identical test; treat it as its own design task, not a quick win.

## Steps (each independently green)
- **R4.1 — Find the parallel seam.** Identify independent layout subtrees (flex/grid
  children whose sizes don't depend on siblings once the parent's available space
  is known) — the natural fork points. Document the dependency rule. *(Blocked on
  the taffy constraint above — needs the multi-`TaffyTree` split first.)*
- **R4.2 — Threshold + scoped fork.** Below N nodes, stay serial (threading
  overhead isn't worth it — same policy as `scene.rs`); above it, lay out
  independent subtrees on scoped threads, join, then finalize parent-dependent
  positions on the main thread.
- **R4.3 — Determinism.** Guarantee identical output regardless of thread count
  (no order-dependent accumulation); test the same tree at 1 vs N threads for
  byte-identical layout. Compose with R2: threaded layout runs only on the dirty
  subtrees when incremental.
- **R4.4 — Benchmark.** Add a large-tree (10k+ node) layout bench; record serial vs
  threaded against the `01 §9` budgets on this box.

*Acceptance:* a 10k-node layout is measurably faster threaded with **byte-identical**
output to serial; small trees are unaffected (stay serial); no new deps.

---

# Sequencing

```
R0  Golden guardrail (diff harness: GPU≈CPU, damage==full)  ── gates everything
 │
 ├── R1  GPU surface backend  (top priority)
 │     R1.1 surface → R1.2 rects-live → R1.3 paths(lyon) → R1.4 gradients
 │       → R1.5 layers/blend/backdrop → R1.6 HiDPI
 │                         │ (R1.5 scissor)        │ (text)
 │                         ▼                        ▼
 ├── R2  Dirty flags / incremental / damage        R3  Glyph atlas
 │     R2.1 mark → R2.2 relayout_subtree            R3.1 per-glyph cache (CPU)
 │       → R2.3 retained DL + damage                  → R3.2 atlas → R3.3 GlyphRun
 │       → R2.4 present tiles (CPU now / GPU after R1.5)  → R3.4 retire sprites
 │       → R2.5 determinism gate                       → R3.5 HiDPI subpixel
 │
 └── R4  Multi-threaded layout (independent; compose with R2's dirty subtrees)
```

- **R0 lands first** — it's the safety net for all the rest, and it's pure
  in-sandbox test work (no deps, no device required to *write*; GPU half skips
  cleanly without an adapter).
- **R1 is the headline** and the longest; its sub-phases each ship a visible win
  (hello → svg → palette → modal → HiDPI).
- **R2's CPU half** (`write_texture` sub-rect) can land in parallel with R1 once
  R2.3 exists; **R2's GPU half** waits on R1.5.
- **R3.1** (per-glyph CPU cache) is a quick early win, independent of the GPU;
  R3.3+ wait on R1.2 (surface + instancing).
- **R4** is independent of R1–R3 and can be picked up any time; it only *composes*
  with R2 (run threaded layout on dirty subtrees).

# ADR-003 implications

- **R0, R2, R4: no new deps.** Pure std + existing `wgpu`/tiny-skia. R4 copies the
  `std::thread::scope` model already in `scene.rs` (no rayon).
- **R1.3: `lyon`** (path tessellation) — one decision-log entry. The rest of R1
  uses the `wgpu` already in `lumen-shell`/`lumen-render`.
- **R3:** atlas packing can be hand-rolled (no dep) or use a tiny packer crate
  (ADR if chosen); glyph rasterization reuses the existing swash path.

# Risks & mitigations

- **GPU≠CPU pixel drift.** AA/gradient/text differ subtly between tiny-skia and a
  GPU pipeline. Mitigation: R0's *tolerance-based* differential (documented max
  delta), not byte-equality, for GPU; byte-equality only for the damage path
  (same renderer).
- **Damage correctness (R2).** A missed dirty mark = stale pixels. Mitigation: the
  R0.3 equivalence test + a fuzz of random single-signal edits asserting
  damage==full; ship behind a flag with full-repaint fallback until the fuzzer is
  clean.
- **No GPU on CI.** Mitigation: GPU differential tests *skip with a logged reason*
  when no adapter; this dev box has a real GPU (RTX 4070 + lavapipe) for local
  verification.
- **Threaded-layout nondeterminism (R4).** Mitigation: independent-subtree-only
  parallelism + a 1-vs-N byte-identical test; never parallelize sibling-dependent
  sizing.
- **CPU renderer regressions.** The whole plan's guardrail (R0) exists precisely so
  the deterministic reference (Rec #5) can't silently rot.

# Acceptance (whole plan)

1. **CPU stays golden:** R0 harness in CI; GPU within tolerance, damage byte-exact;
   GPU-absent skips cleanly.
2. **GPU surface (Rec #1):** all example screens render live through `Wgpu`
   on a real surface; full `DrawCmd` set (rect/path/gradient/layer/blend/backdrop)
   covered.
3. **Incremental (Rec #2):** single-signal edits relayout only the dirty subtree
   and repaint only the damage rect (counter-asserted), byte-identical to a full
   repaint; idle pumps present nothing.
4. **Glyph atlas (Rec #3):** text renders via `GlyphRun` on the GPU atlas;
   per-glyph caching bounds re-raster on edits.
5. **Threaded layout (Rec #4):** 10k-node layout faster threaded, byte-identical to
   serial; small trees unaffected.
</content>
</invoke>
