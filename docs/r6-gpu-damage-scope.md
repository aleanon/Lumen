# R6 (damage on the GPU present path) — real scope, and two corrections

*2026-08-08. Investigation, not implementation. R6 is larger than the campaign
plan scoped it, and two of its stated premises are wrong.*

## Correction 1: the Critical finding was overstated

`.ai_docs/review-2026-08/01-performance.md` lists as Critical #1 that **damage
is computed and then thrown away** on the GPU present path, citing this comment
in `app.rs`:

> `// granularity is ignored — the GPU renders the whole frame anyway`

The comment is precise; the finding generalised it too far. Damage's **binary**
signal is used, and correctly:

- `app.rs:985` — `painted: self.last_damage != Damage::None`
- `lumen-shell/src/lib.rs:778` — `if stats.painted || resized || force { … present }`

So a frame whose damage is `None` is **never presented at all** — no swapchain
acquire, no encode, no submit. The coarse win the campaign attributed to R6.1
("early-return on `Damage::None`") already exists, one level up, and R6.1 as
written is a no-op.

What is genuinely discarded is damage's **region**: when anything changed, the
whole frame is re-encoded regardless of how little moved. That is a real cost,
and it is what R6 must actually fix — but it is a *granularity* problem, not the
"computes damage and ignores it" characterisation.

## Correction 2: R6.5 is not a one-line change

The plan says *"raise atlas pages from 1 (`gpu.rs:897`) — one-line change to 4
pages plus an age-based eviction"*. It cannot be.

`GlyphAtlas::new(ATLAS_SIZE, 1)` is capped at one page because the GPU side is a
**single-layer 2D texture** (`depth_or_array_layers: 1`, `gpu.rs:852`), and the
instance data carries only `slot.x`/`slot.y` normalised over one page
(`gpu.rs:1545`) — there is no page index anywhere in the vertex stream or the
shader. Raising `max_pages` would place glyphs on page 1+ that sample the
**wrong texels**: a correctness bug that renders as garbled text, not a capacity
win.

Doing it properly means a texture array (or a larger single page), a page index
threaded through `GlyphInstance`, and a shader change to sample the array layer.

## Why the rest of the chain is blocked on that

R6's phases are ordered `R6.4` and `R6.5` **before** `R6.3`, and the ordering is
load-bearing:

- **R6.2** (cull the display list to the dirty region) is *not safe on its own*.
  The current pass clears to background and redraws; culling without also
  preserving the previous frame turns every undamaged pixel into background.
- **R6.3** (scissor + `LoadOp::Load`) is what makes culling correct — but
  loading last frame's content requires that content to still exist, which needs
  **R6.4** (pooled, persistent render targets; today `encode_root` creates a
  fresh resolved/MSAA texture per layer per frame and drops it).
  **R6.4 landed 2026-08-08**: `TargetPool` keys targets on
  `(width, height, samples)` and derives availability from `Rc::strong_count`,
  so a target an in-flight frame still holds cannot be reissued. Measured 2
  targets across 13 frames against 18 unpooled. Sizes unused for 120 frames are
  evicted, which is what stops a resize drag pinning one target per intermediate
  size. Targets now *persist*, so R6.3 has the content it needs to `Load` — the
  remaining blocker there is the atlas clear (R6.5 made it 4× rarer, not
  impossible) and the `get_current_texture()` content guarantee.
- And `LoadOp::Load` is unsound while the atlas can **wipe itself**: on overflow
  today the whole atlas is cleared (`gpu.rs:1102`), which invalidates glyphs the
  retained target still shows. Hence **R6.5** first.
  **R6.5 landed 2026-08-08**: the atlas is a 4-layer texture array, `page` rides
  in the instance stream as a flat varying, and the glyph pipeline has its own
  bind-group layout (view dimension is part of a layout, so it could not keep
  sharing `image_bgl`). Clears are now 4× rarer rather than eliminated — the
  soundness argument for R6.3 still needs the clear to be *handled*, not just
  made less frequent.

~~There is also an unresolved correctness question the plan already flagged:~~
**Resolved 2026-08-08 — it does not apply to Lumen.** `get_current_texture()`
indeed guarantees nothing about the acquired image's prior content, but Lumen
never renders into it: `present_to_surface` encodes into an offscreen root and
then blits with `LoadOp::Clear`, fully overwriting the swapchain image every
present. R6.3 would `Load` from the *root*, which Lumen owns and which persists
since R6.4. The constraint that actually shapes R6.3 is MSAA — both the
multisample attachment and the resolve target must be retained and loaded. See
`docs/r6.3-design.md`.

## Revised estimate and recommendation

R6 is not a plumbing task. It is: atlas texture-array support (+ shader), render
target pooling with a lifetime contract, swapchain content semantics, and only
then scissored partial redraw — each with GPU-visible correctness risk.

**~~And it is currently ungateable.~~ Resolved 2026-08-08.** The claim was that
the lavapipe job "fails on gradients", leaving R6 with no way to prove it had
not broken rendering. The gradient failure was never lavapipe's: `Backends::all()`
was resolving to the **OpenGL** adapter, where `textureSample` of the gradient
ramp returns zeros — a real shipped defect that silently dropped every gradient
on any machine wgpu resolved to GL. `Wgpu::new` now prefers PRIMARY. On real
lavapipe the scene renders and matches the CPU better than any other in the
corpus (ΔE 0.0039), and the full 16-suite render set passes there under
`LUMEN_REQUIRE_GPU=1`. See `docs/gl-gradient-defect.md`.

Recommended order: ~~fix the lavapipe gradient failure~~ (done) → broaden GPU
parity → R6.5 (atlas array) → R6.4 (target pooling) → R6.3 (scissor + Load) →
R6.2 (cull). R6.1 is retired as already-implemented.

## What this does not change

Nothing here weakens the case for R6 — a full re-encode for a one-pixel change
is real waste. It changes the *size* of the work and the *order* it must happen
in, and it removes one item that was already done.
