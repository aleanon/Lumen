# Path to A+ performance — reachability assessment

*Research pass, 2026-08-07. Scope: is A+ performance reachable for Lumen, and
at what cost? Grounded in `.ai_docs/review-2026-08/01-performance.md` (D+,
31 findings), `00-SYNTHESIS.md`, `docs/plan-incremental-path.md` (CP-series),
`docs/results-node-cost-n0.md` (the only trustworthy numbers in the repo),
`docs/plan-rendering-performance.md`, `docs/plan-retained-pipeline.md`,
`docs/plan-fine-grained-view.md`, the architecture review (`05-architecture.md`),
ADR-002/004/006/007, and source read directly (`crates/lumen-layout/src/tree.rs`,
`crates/lumen-widgets/src/app.rs`, `crates/lumen-render/src/{gpu,scene,diff}.rs`,
`crates/lumen-text/src/lib.rs`, `crates/lumen-core/src/identity.rs`), plus
external research on vello, Flutter Impeller, Skia Gold, WebRender/Servo, and
GPU-testing-in-CI practice. Per the brief: this assesses *reachability*, not
desirability, and follows the evidence to "not reachable without giving up a
pillar" if that is what it shows. It is not that — the conclusion is
conditional-yes — but the conditions are real and stated plainly.*

---

## Verdict

**Conditional yes.** A+ performance is not blocked by anything irreducible in
Lumen's architecture. Every mechanism the D+ grade cites as broken (damage
discarded on GPU present, memoization costing more than rebuild, animation
disabling memoization app-wide, no virtualization by default, O(n) semantics
rebuild on hover) is a **bookkeeping defect in a sound design**, not a
structural ceiling — this matches the approved campaign's own read
(`zippy-dancing-allen.md`: "there is no architectural reason all five cannot
reach A"). Two things stop this from being an unqualified yes:

1. **The campaign's own stated outcome is B/B+, not A+**, and it stops there
   *deliberately* — CP5 is a gate with "stop" as a permitted, plan-sanctioned
   outcome, and the campaign explicitly declines the additional work (real
   O(changed) layout, competitive benchmarking, GPU backend consolidation)
   that separates B/B+ from A+. Reaching A+ means picking up exactly the
   work the approved plan puts down. That is available, costed below, and
   not free.
2. **A+ cannot currently be *asserted*, only pursued** — Lumen has never
   benchmarked itself against the peer set an A+ claim implies (Slint,
   Makepad, egui, Flutter release builds; see `01-performance.md` "Competitive
   positioning"), has no real ARM hardware measurement (only an x86_64-under-
   KVM floor), and its only claimed "peak performance" precedent
   (`docs/comparison-gtk-mintupdate.md`) is explicitly self-disclaimed as not
   evidence against real compiled competitors. An A+ grade without that
   comparison is a self-assessment, not a verified one — closing this gap is
   part of the path, not a footnote.

**The condition, stated plainly:** A+ is reachable *if* the team (a) commits
the CP6 persistent-tree work (the single largest remaining item, gated
behind CP5's own measurement, with real odds CP5 says "stop" — see Blocker
analysis §2), (b) validates on real ARM/mobile hardware rather than the
emulator floor already flagged as not-an-ARM-number, and (c) actually runs
the competitive comparison the review found conspicuously absent. None of
these is blocked by anything in the codebase; all three cost real calendar
time and carry real result-risk (the CP6 gate might legitimately say no; the
competitive numbers might legitimately come out unfavorably). That risk is
why this is "conditional," not "yes."

**No stated pillar needs to be abandoned.** Determinism (ADR-002), the
single-source-of-truth observability model (ADR-009), and the agent-first
API surface all survive the full path below intact. What gets spent is
calendar time and architectural surface area (a persistent tree with
generational-handle churn, `Rc<LayoutStyle>` at ~552 call sites, a breaking
agent-protocol version bump) — engineering cost, not a pillar trade. See
*What must be given up* for the honest accounting of that cost.

---

## The rendering architecture decision

This is the most consequential section, because it is the one place a wrong
call would be genuinely hard to undo. The recommendation: **keep two
backends — CPU deterministic reference, GPU for users — but replace the
hand-rolled GPU backend with vello,** on a track decoupled from and not
blocking the rest of the performance path.

### What Lumen already has right

ADR-002's decision (tiny-skia is the renderer of record; GPU is compared
against it) is not a compromise Lumen was forced into by immaturity — it is
**the same architecture every serious graphics project this size converges
on**, and Lumen has already built the two supporting mechanisms correctly:

- A **perceptual-tolerance GPU parity harness** already exists and is
  already in use: `Tolerance::PARITY` (`crates/lumen-render/src/diff.rs:23`)
  is Oklab ΔE ≤ 0.04 on ≥ 99.5% of pixels, checked by `cpu_vs_gpu.rs`. Android
  on-device goldens use a separate, wider tolerance (ΔE ≤ 2.0 on ≤ 0.1% of
  pixels, `.ai_docs/07-decision-log.md:121`). This is not a future plan —
  it's shipped, and it is structurally identical to what Flutter/Skia do (see
  below).
- The plan to run GPU tests in CI (`GX0`, `zippy-dancing-allen.md`) is
  **lavapipe on `ubuntu-latest`** — a software Vulkan implementation, not
  real hardware. This is, independently and unknowingly, the exact same
  strategy vello's own CI uses (confirmed by reading vello's CI workflow
  directly: `mesa-vulkan-drivers` + `xvfb`, gated by `VELLO_CI_GPU_SUPPORT`)
  and the same strategy wgpu's own test suite uses (lavapipe for Vulkan,
  llvmpipe for GLES). Lumen is already doing the right thing here without
  having had to research it.

### Is GPU determinism achievable as the renderer of record? No — and neither vello nor anyone else has it

Researched directly against vello, Flutter's Impeller, Skia/Ganesh, and
WebRender/Servo — the answer is uniform and unambiguous: **no production
GPU-rasterization project claims or relies on bit-exact output across GPU
vendors/drivers**, and the ones with the deepest investment in this problem
(Skia, Flutter) instead built infrastructure to *manage* the variance rather
than eliminate it:

- **Skia Gold** stores baselines per (OS, architecture, backend) combination
  — not one universal golden image — and every commit produces
  &gt;500,000 comparison images across that matrix, triaged as positive
  (accept as a new legitimate baseline) or negative (real bug). Chromium's
  GPU pixel tests (same Gold backend) go further: multiple approved images
  per test plus a documented `matching_algorithm` for genuinely fuzzy
  comparison.
- **Flutter's Impeller golden tests explicitly add GPU model as a test
  dimension** (`AddDimension("gpu_string", ...DescribeGpuModel())`) and, per
  `flutter/engine#40824`, apply a **global fuzzy threshold** — up to 1% of
  pixels may differ, by up to 4 (raised to 8 for noisier tests) color-component
  units — specifically because, in a contributor's words, "we don't want this
  test to be tripping up all the time." Flutter does not attempt cross-GPU
  bit-exactness; it manages the noise.
- **Firefox's reftest harness** has had a `fuzzy(maxDelta, maxPixelCount)`
  annotation for years, now parametrized per backend
  (`fuzzy-if(webrender,0-1,0-3)`) — the same "this backend is noisier, widen
  its tolerance" pattern, generalized across Gecko's whole rendering stack,
  not just WebRender.
- **vello itself is not deterministic across GPU vendors, and does not claim
  to be.** It shipped a real cross-frame non-determinism bug in its stroke
  miter-join math as recently as `vello#1323` (a near-zero cross-product
  destabilized downstream pipeline stages; the reviewer's own comment flagged
  that "this could easily hide similar errors" — i.e. even vello's own
  maintainers treat this class of bug as open, not closed). vello's CI
  validates GPU-path correctness against **lavapipe**, not real hardware. And
  critically: **vello_cpu is not the same algorithm as GPU vello** — it's a
  separately-implemented sparse-strip CPU rasterizer, not the same shader
  code executed on CPU, so it is not guaranteed pixel-identical to the GPU
  path even in principle. Adopting vello does not hand Lumen a free
  deterministic reference; it hands Lumen the same "two renderers must agree
  within tolerance" problem it already has, solved by a team with deeper
  graphics expertise but not solved *away*.

**Conclusion on determinism:** fixed-function pipelines, integer math, or a
from-scratch deterministic GPU renderer are not a real option at Lumen's
scale — every project with more graphics engineering investment than Lumen
(Skia, Flutter, Mozilla) chose tolerance-based testing over trying to
eliminate GPU variance at the source, and vello (Lumen's most plausible GPU
upgrade path) has an open, recently-patched non-determinism bug of exactly
the kind this question worries about. The CPU-reference architecture ADR-002
already chose is the industry answer, not a stopgap.

### Is perceptual-diff golden testing (ΔE thresholds) sufficient, letting CPU be dropped entirely? No — for a reason specific to Lumen's audience

The general technique is sufficient (Skia/Flutter/Firefox prove it works at
far larger scale than Lumen). The reason to keep CPU as the *primary* golden
path is Lumen-specific, not a technique limitation:

1. **Agent self-verification needs a ground truth that is identical across
   every sandbox the agent runs in**, including sandboxes with no GPU at all.
   A headless CI container, a cloud dev sandbox, or a CI runner with only
   lavapipe cannot produce the same pixels a developer's RTX 4070 produces —
   under a GPU-primary model, "does this look right" becomes "does this look
   right *on this specific GPU*," which is a materially worse guarantee for
   an agent that cannot itself judge whether a ΔE-sized drift is a real
   regression or hardware noise. A byte-exact CPU raster answers that
   question with a diff of zero bytes; a perceptual GPU raster answers it
   with "probably."
2. **Bisection breaks.** ΔE-tolerant comparison is fine for "does this PR
   look right" (Flutter/Skia's use case, with humans triaging Gold). It is
   much weaker for "did this specific commit regress this specific pixel,"
   which is what `doc_shot`, the widget golden-image suite, and
   `assert_view_coherent` all currently do with byte equality. Converting the
   whole corpus to tolerance-based assertions is a real loss of
   regression-catching precision (a bug smaller than the ΔE threshold is
   invisible by construction), not a neutral format change.
3. **It would remove, not add, infrastructure Lumen needs anyway.** Lumen
   already needs a GPU parity harness (`cpu_vs_gpu.rs`) for the reasons Skia/
   Flutter/Firefox demonstrate. It does *not* need to give up byte-exact CPU
   goldens to have that; it already has both.

**Recommendation: keep two backends, permanently, but change what the GPU
backend is built on.** The hand-rolled `gpu.rs` (~3,000 LOC) independently
reinvents several things vello's architecture solves by design: no
persistent buffers (F5), no glyph-run batching across runs (F9), a
single-page glyph atlas with clear-the-world eviction (F15), an unbuilt lyon
tessellation cache (R1.3). vello (or vello_hybrid, given wgpu 22 vs vello's
current wgpu pin needs checking at adoption time) is built by a team whose
whole job is exactly this class of problem, and it already shares Lumen's
`kurbo`/`parley` stack (confirmed: Xilem's Masonry consumes kurbo *through*
vello, the same primitives Lumen already depends on) — so adoption is a
renderer-backend swap, not a new geometry model. This is **exactly the
evaluation ADR-006 already scoped** ("Vello-style compute rasterization is an
M4 evaluation, not a dependency") and **exactly what `scene.rs`'s own doc
comment already names as the intended production GPU path** — the
architecture already points here; A+ requires walking through the door the
project already drew.

**What this is not:** a reason to unify onto one backend, and not on the
critical path for the rest of this document's performance recommendations.
Most of what vello would fix (F5, F9, F15, R1.3) is independently fixable in
weeks without vello (see M-D in the approved campaign); vello is worth
pursuing because it closes those gaps *and* removes a maintenance burden
going forward, not because performance is blocked without it. Treat it as a
parallel track (see *The path*, step 7), gated on its own maturity check —
as of the research pass, vello's GPU backend is at v0.9 (not 1.0), Hybrid is
"beta quality" with "rough edges" and no API-stability guarantee. ADR-R1's
own revisit trigger ("Vello reaches a stable release whose WebGPU compute
baseline the supported platforms meet") is not yet satisfied and should gate
the decision to actually cut over, not just evaluate.

### Two-backends-forever, refined

The honest framing is not "CPU vs. GPU as a tension to resolve" but "CPU is
the *test* backend, GPU is the *product* backend, and they were never
supposed to converge." Every peer project with more resources than Lumen
reached the same conclusion. The only actionable change is *what the GPU
backend is implemented on top of*, and that choice should be made on its own
timeline (vello maturity), not gated into the performance-A+ critical path.

---

## Blocker analysis

Per subsystem: **unfinished work** (build it, no design questions),
**revisitable decision** (an ADR/architecture choice that could change), or
**irreducible** (a real ceiling given the current dependency stack).

### 1. Incremental/retained pipeline — unfinished work, with one revisitable decision inside it

The CP-series (`copy_node`/`copy_span` cost) is unambiguously unfinished
work: three named, measured, non-architectural cost sources (O(scopes²)
nested-span scan, 8 hashmap ops per copied node, SipHash overhead). Fixing
these (CP0–CP4) is scoped, low-risk, and gets the scoped path to parity with
a flat rebuild (ratio ≤ 1.0, retiring the earlier ≤0.5 target as
unsupported by any measurement). This alone does **not** reach the
theoretical floor.

**The theoretical floor for "1 of 500 nodes changed" is O(1)** — SolidJS,
Leptos, and Dioxus's fine-grained signal graphs get exactly this for the
*view* side: a signal write patches the one DOM/render-tree node that reads
it, with no re-diff of anything else. Lumen's F0–F5 work
(`plan-fine-grained-view.md`) already built the *attribution* half of this
correctly — `write_one_of_many_reruns_exactly_one_scope` is proven — the gap
is entirely in the *cost per patched node*, which is what CP1/CP2 fix, and
in **layout**, which nothing in the CP-series touches.

**This is the revisitable-decision layer, and it is layout, not reactivity
— but the finding is better news than it first looks, and is spelled out in
full in §2 below rather than duplicated here.** In short: `taffy` already
ships real per-node incremental layout (a `mark_dirty` + cache mechanism,
proven by taffy's own PR #246 collapsing a 10,000-node worst case from 17s
to 3ms) — Lumen just never keeps a tree around long enough to use it,
because `rebuild_inner` mints a fresh `TaffyTree` every rebuild. So CP6
(persist `Tree`/`LayoutTree`) is not building new layout-incrementality
machinery from scratch; it's wiring up to machinery that already exists.
The one genuinely irreducible piece — flexbox/grid free-space distribution
coupling all siblings in a container, which no conformant engine (taffy,
Flutter's `RenderFlex`, anyone's) escapes — is exactly the shape
`VirtualList` already sidesteps by using absolute positioning instead of
flex flow. See §2 for the full evidence and the re-ranked options table.

**Verdict:** unfinished work for the ratio fix (CP1–CP2); unfinished-but-
gated for the retained tree (CP6) — cheaper on the layout side than the
original framing suggested, since it inherits taffy's existing cache
rather than building new incrementality; the residual flexbox-coupling
ceiling is irreducible but already correctly worked around by
`VirtualList`, making the practical fix a packaging problem (§4), not a
new layout engine.

### 2. Layout — mostly unfinished work (wire up what taffy already has), one irreducible sliver correctly worked around

Corrected against taffy's own source (not just Lumen's docs describing it):
`taffy::TaffyTree` already has a real per-node incremental-layout cache,
invalidated by `mark_dirty` walking up the ancestor chain with early-stop,
and `compute_layout` already skips descending into subtrees whose cache
still matches. This is proven in production terms by taffy's own PR #246
(a 10,000-node/14-level layout dropping from **17s to 3ms**). Lumen's
"whole-tree serial solve" behavior, as diagnosed by the review, is real —
but it's a consequence of Lumen minting a **fresh `TaffyTree` on every
rebuild** (confirmed directly: `rebuild_inner` replaces `self.tree` with
`Tree::new()` and calls `LayoutTree::new()`), not a limitation of taffy
itself. `LayoutTree::set_style` — the API that would call taffy's
`set_style`/`mark_dirty` on a *persisted* node — has zero production call
sites. **The cache exists; Lumen has never kept a tree around long enough
to benefit from it.**

Two comparison points, now independently sourced, sharpen where the real
ceiling is:

- **Flutter's `RenderObject`** achieves incrementality via
  `markNeedsLayout()` propagating up only to the nearest **relayout
  boundary** — a node whose size can't be affected by its subtree (tight
  parent constraints, or `sizedByParent`). This is architecturally close to
  what taffy's cache-with-early-stop already does; the frameworks differ in
  whether the mechanism is documented/named as a public contract (Flutter's
  is; taffy's is an internal optimization with no roadmap item framing it
  as "incremental layout" — issue #345, taffy's own roadmap, has no such
  item).
- **Jetpack Compose's `LayoutNode`** and (lower-confidence, third-party-
  sourced) **SwiftUI's Attribute-Graph-backed layout cache** are the two
  fine-grained-reactive frameworks that, unlike SolidJS/Leptos/Dioxus-web,
  actually own their layout pipeline end-to-end and had to build (and did
  build) persistent, incrementally-invalidated layout trees — the correct
  peer set for "is Lumen's layout incrementality story competitive with the
  best fine-grained frameworks," not Solid/Leptos, which get to outsource
  the problem to a browser.

**The genuinely irreducible piece is flexbox/grid free-space distribution
coupling siblings within one container** — taffy's cache key includes
`available_space`, so one child's content-size change forces a
redistribution pass (and cache miss) across every sibling in that flex/grid
container, even though their own subtrees are untouched. This is inherent
to the CSS box model, not an engine choice: Flutter's `RenderFlex` has the
identical coupling for flexible children. No layout engine — taffy, a
custom replacement, or Flutter's own — makes "1 of 500 flat flex siblings
changed" O(1); it's O(siblings-in-container) at best, by spec.

**And that's exactly the shape `VirtualList` already sidesteps, correctly
and non-accidentally.** Its rows use `Position::Absolute` with an explicit
pixel offset (confirmed by direct read), removing them from the parent's
flex flow entirely — no shared free-space pass to invalidate. This is the
same move as Flutter's `SliverList` (never lay out off-screen children;
don't participate in a shared distribution at all). Lumen already has the
right primitive for the case that's actually irreducible; the review's F4
finding is that it isn't the default, which is a packaging problem (§4),
not a missing algorithm.

**Options, re-ranked given the corrected picture:**

| Option | What it buys | Cost | Status |
|---|---|---|---|
| **(a) CP6 wires `LayoutTree` to persist + calls `set_style` instead of minting fresh nodes** | Inherits taffy's *existing* cache — the 17s→3ms class of win, for free, on any non-flex-coupled change | Already costed inside CP6 (Phase 1) — this is not additional work beyond what the incremental-pipeline path already requires | Gated behind CP5, per the incremental-pipeline analysis above |
| **(b) Promote `VirtualList` as default** | Sidesteps the one genuinely irreducible case (flex-sibling coupling) for the shape that actually hits it (long flat lists) | Low — already built, already scoped as VL1 | Scheduled in the approved campaign's M-D |
| **(c) Multi-`TaffyTree` split (R4)** | Isolates large *non-list* regions (e.g. an editor canvas, a dashboard with independent panels) that aren't naturally virtualizable but are independent for layout purposes | Medium — real layout-driver change, needs a 1-vs-N byte-identical proof | Correctly parked (10.2% of frame today); re-measure after (a) lands, per ADR-R1's own written trigger |
| **(d) Contribute a named/public incremental-layout contract upstream to taffy** | Turns today's internal-optimization cache into a documented guarantee other taffy consumers (plausibly Dioxus's native renderer, which per ecosystem knowledge also sits on taffy) would benefit from too | High, outside Lumen's control, no roadmap signal from taffy's maintainers | Not started; not escalated; taffy's multithreading issue (#27) has sat open and "controversial" since 2022 — a signal that upstream engineering-effort asks move slowly on this project |
| **(e) Replace taffy** | Full control | Highest cost, contradicts ADR-004, forfeits taffy's flex/grid correctness maturity, and would still hit the same flexbox-coupling physics | Not proposed; not worth it |

**Verdict: mostly unfinished work.** (a) is already inside CP6's scope, not
an addition to it — this *lowers* the effort estimate for reaching
competitive layout incrementality versus treating it as a separate
subsystem. (b) is nearly free and already scheduled. (c) stays correctly
parked. (d)/(e) are not worth pursuing given (a)+(b) already close most of
the gap.

### 3. Text — unfinished work, cheap

`ShapeKey` (`crates/lumen-text/src/lib.rs:207`) includes the full `text:
String`, confirmed directly. Two independent, already-diagnosed defects:
whole-document reshaping on every keystroke (F12: `text_field.rs`/
`widgets_m4.rs` restringify the entire buffer before shaping) and an
uncached `layout()` call on every pointer-move during drag-select (F13,
bypassing the cache the paint path already warmed).

**Best-in-class, sourced directly from Blink's own text-stack docs, is
narrower than what Lumen's own fix proposes.** Chromium's
`CachingWordShaper` documents its granularity explicitly: "the basic unit
for storing shaping results in a cache is a word, separated by spaces...
for CJK text, each individual CJK character is treated as a word," keyed
additionally on the active font-fallback-list state, with a *separate*
lower-level cache (HarfBuzz's own shape-plan cache) avoiding shaper-setup
cost on top of that. Word/character granularity is one level finer than
Lumen's own proposed fix (split on `\n`, key per line) — a single edit
invalidates one word, not one line. **Word-granularity is materially
harder to retrofit** than line-granularity, though: it requires decoupling
shaping from line-breaking (a wrapped multi-line paragraph's line breaks
depend on shaped word widths, so word-level caching interacts with reflow
in a way whole-line caching doesn't), which is a bigger design task than
F12's proposed fix. **Recommendation: land the line-granularity fix first
(F12's own scope — it does not require new architecture, `ShapeKey`
already supports arbitrary granularity, it's just handed the wrong scope
today) as the pragmatic 80% win; treat word-granularity as a stretch goal
past the A+ path in this document, gated on evidence that line-granularity
isn't enough for a real long-line-editing workload.** F24 (the
`to_string()` allocation on every cache *hit*, not just miss) is a smaller,
independent fix (borrow-friendly raw-entry lookup).

Separately: Lumen's per-*glyph* raster cache (R3.1, keyed on
`(font, glyph id, size, subpixel bucket)`, feeding the GPU atlas) already
matches the pattern browsers and native text stacks (e.g. Zed/GPUI's
`(glyph_id, font_id, size, offset)` → bitmap cache) use at the
rasterization layer — this part of the stack is already right; the gap is
entirely at the shaping-cache-key layer above it.

**Verdict: unfinished work, low-medium effort, no architectural blocker.**
The planned fix (line-granularity) is achievable now; the theoretical
best-in-class (word-granularity) is a larger, separable follow-on.

### 4. Scroll/virtualization — unfinished packaging, not a capability gap

`VirtualList` (`crates/lumen-widgets/src/widgets_m1.rs:543-623`) measures
1.15ms/frame for 1M rows — genuinely competitive: in the same neighborhood
as Qt's index-based `QAbstractItemView` (native, no per-row widget
allocation) and comparable to what windowing libraries achieve for Flutter's
`ListView.builder` or web `react-window`-class approaches at similar row
counts. **The gap is entirely that `VirtualList` isn't the default** —
`Scrollable` (the discoverable API) lays out and paints every child every
frame regardless of viewport, by its own doc comment's admission. This is
F4 in the perf review, already correctly triaged as "the cheapest fix in the
review" and already scheduled (VL1 in the campaign).

**Verdict: not a blocker at all. Already A+-capable; only needs promotion to
default + a size-threshold lint on unbounded `Scrollable`.**

### 5. Parallelism — unfinished work, and a real gap between what exists and what an A+ framework would exploit

What's parallelized today: exactly one thing — `cull_visible`
(`crates/lumen-render/src/scene.rs`), a `std::thread::scope`-chunked
viewport cull above an 8,192-item threshold, with an order-preserving
concatenation so output is deterministic regardless of thread count. This
same pattern (embarrassingly parallel, deterministic-by-construction,
threshold-gated, no rayon per ADR-003) is the template for everything below,
and none of the below reuses it yet:

- **Style resolution** — independent nodes' rule matching is embarrassingly
  parallel; not threaded.
- **Display-list emission** — for the *cold-build* case specifically (a
  large static tree on first paint or after a full rebuild), independent
  subtrees could emit into separate `Vec<DrawCmd>` buffers and concatenate
  in document order, exactly like `cull_visible`'s chunking. Not attempted.
  (The steady-state case is better served by R5's fragment-caching, which
  avoids the work rather than parallelizing it — the right call there.)
- **Text shaping** — parley/swash shaping of independent runs has no shared
  mutable state beyond the cache; this is a well-known browser optimization
  (shaping cold text off the UI thread/pool). N0's own frame breakdown puts
  shaping+glyph-raster at ~21% of a 500-node frame — the largest single
  named component — and none of it is threaded.
- **GPU command encoding** — batching by draw-cmd type already exists (a
  real win); glyph runs specifically don't batch across runs (F9, a fixable
  gap, not a parallelism gap) and CPU-side command-buffer *recording* for
  large scenes isn't split across threads (a standard Vulkan/wgpu pattern
  wgpu itself supports).
- **Layout** — per §2's corrected finding, threading isn't the primary
  lever here (taffy's own solve is single-threaded, and taffy's own
  multithreading issue, #27, has sat open and unresolved/"controversial"
  since 2022 — confirming upstream parallel-layout is a slow path, not a
  quick win, if pursued at all). The higher-leverage move for layout is §2's
  (a) — actually retaining the tree so taffy's existing *incremental* cache
  fires — which is a skip-the-work lever, not a do-the-work-faster one.

**Verdict: unfinished work across the board except layout parallelism
specifically, which is a low-value target even upstream (taffy's own
maintainers have left it unresolved for years). The `scene.rs` pattern is
proven, in-tree, and directly reusable for style/DL-emission/shaping — this
is some of the cheapest available leverage in the whole path, and it's
currently unused outside one function.**

### 6. The observability tax — unfinished work, not a permanent floor

`build_semantics` (`crates/lumen-widgets/src/app.rs:4277-4324`, confirmed by
direct read) is a full O(n) recursive walk with a `format!("{:?}", role)`
allocation and several `.clone()`s per node, called from `restyle_visual` on
**every hover/focus/press** — not just structural rebuilds, directly
contradicting the doc comment two lines above it. This is the review's F11
and is independently corroborated by the architecture review's system
diagram. The campaign's own investigation (`zippy-dancing-allen.md`)
establishes the important qualifier: **the tree's existence is not the
cost — its eager, ungated, full-walk *construction on every restyle* is.**
Every real reader is already conditional (AccessKit only runs inside
`update_if_active`, gated on an AT client; the agent surface is feature-
gated out of release builds entirely); only `build_semantics`'s call site
is unconditional. Fixing it (OB1: static role names instead of `format!`;
OB2: patch the flipped node's state into the existing `sem_root` instead of
rebuilding; OB3: gate the dependency index behind the `snapshot` feature,
already verified as having exactly one production reader) is scoped,
moderate effort, and already scheduled in the approved campaign's M-D.

**At A+, does this matter, and can it be incremental?** Yes it matters —
hover/restyle is the highest-frequency interaction event in a live UI, so an
O(n) cost there is paid far more often than its low per-node cost would
suggest, and it directly undercuts the framework's own headline claim ("no
rebuild, no relayout, no scope re-run" on restyle — true for paint, false
for semantics, in the same function). Yes it can be made incremental — the
fix is exactly the same "patch the one changed node in the retained
structure" pattern the render side already does for paint-only bindings
(F3.4); there's no reason semantics can't receive the same treatment, and
OB2 already proposes it.

**Verdict: unfinished work, not an architectural tax.** Once OB1–OB4 land,
an app with no AT client and no agent attached pays ≈0, and an app *with*
one attached pays O(changed) instead of O(tree) on the highest-frequency
path — closing the gap between what ADR-009 promises and what the restyle
path currently delivers.

---

## The path

Ordered, with prerequisites and effort as **estimates** (labeled), building
on the approved campaign's M-A→M-F structure rather than replacing it — this
path *is* that campaign, extended past its own stated stopping point. Sizes
follow the project's own S/M/L/XL convention; person-month figures are my
translation of that sizing, not derived from any measurement, and are stated
as ranges to reflect real uncertainty.

```
Phase 0 (baseline, = the approved campaign)
  M-A (gate + free wins) → M-B (identity break) → M-C (incremental path + CP5 gate)
    → M-D (frame gets cheap: GPU damage wiring, semantics incrementality, VL1)
  ≈ 5–6 person-months. Predicted outcome: B/B+ (the campaign's own estimate).
       │
       ▼  only if CP5 said "yes" to CP6
Phase 1 — CP6: persist Tree + LayoutTree, wire set_style/mark_dirty  L–XL, 2–3 mo
  so taffy's own existing incremental cache actually fires (it is not
  new layout machinery — taffy already has it; this phase is the wiring)
  Prerequisite: CP5 (M-C's gate). One-way door once ID2 (M-B) ships.
       │
       ▼
Phase 2 — VirtualList promoted to default (VL1, already in M-D's        —
  scope, pulled forward in emphasis: this is the actual fix for the one
  genuinely irreducible layout case — flexbox sibling coupling — not a
  new phase's worth of work)
       │
       ▼
Phase 2b — Multi-TaffyTree split (R4), gated on re-measuring       M, 1–1.5 mo
  layout's post-CP6 share of frame against the written trigger — for
  large non-virtualizable independent regions only (editors, dashboards),
  not for lists (Phase 2 already covers those)
  Prerequisite: Phase 1 (measurement only makes sense once the
  build/patch side stops dominating the frame).
       │
  ┌────┴─────────────────────────────────────────┐
  ▼ (parallel, no ordering constraint)            ▼
Phase 3 — Parallelism sweep                    Phase 4 — Text: per-line shaping
  style resolution + cold-build DL emission       (F12/F13/F24)          S–M, 3–4 wk
  + shaping, scene.rs pattern reused    M, 1 mo   Prerequisite: none
  Prerequisite: none (independent of Phase 1/2)
       │                                                │
       └────────────────────┬───────────────────────────┘
                             ▼
Phase 5 — Real ARM/mobile hardware validation           S, 2–3 wk (+ hardware access)
  Replaces the emulator-under-KVM floor with a real number;
  re-run nodecost + perf on-device; this is CP4, already scoped,
  never executed on real hardware.
  Prerequisite: none, but should run before claiming any mobile A+.
                             │
                             ▼
Phase 6 — Competitive benchmarking                      S–M, 3–4 wk
  vs. Slint, Makepad, egui, Flutter release build, on matched
  workloads (the review's explicitly named gap). This is what
  turns "A+" from a self-assessment into a verified claim.
  Prerequisite: Phase 0 complete (comparing against a still-D+
  baseline is not informative).
```

**Parallel, off-critical-path track — GPU backend consolidation:**

```
Track V — vello evaluation → (conditional) adoption
  V.1  Maturity spike: does vello's current release (v0.9 GPU / v0.2 CPU/
       Hybrid as of this research pass) meet ADR-R1's own revisit trigger
       ("a stable release whose WebGPU compute baseline the supported
       platforms meet")?                                          S, 2–3 wk
  V.2  If green-lit: replace gpu.rs's ~3,000 LOC hand-rolled wgpu renderer
       with a vello::Scene-based backend, behind the existing Renderer
       trait seam, gated by the existing R0 cpu_vs_gpu harness (already
       exists, needs no redesign — just a new implementer)         L, 2–3 mo
  Does not block or gate anything in the main path above. tiny-skia
  stays the golden reference unchanged throughout.
```

**Total critical-path estimate: ~11–14 person-months**, spread over
12–18 calendar months for a small team (1–3 engineers), given the
sequencing/gating constraints already built into the campaign (CP5's gate,
ID1's one-way door, the F0 coherence-oracle proof burden on every step of
Phase 1). This is explicitly an estimate translating the project's own S/M/
L/XL task sizes into calendar time, not a bottom-up estimate from a work
breakdown — treat the ratio between phases as more reliable than the
absolute totals.

**What's a prerequisite for what:**
- M-A→M-D (Phase 0) gates everything — nothing past it is checkable without
  the ratio-based perf gates M-A establishes, and CP6 (Phase 1) cannot start
  before M-C's CP5 decision exists.
- Phase 2 (multi-tree layout) is only worth measuring *after* Phase 1, since
  layout's share of the frame is currently a moving target while build/copy
  costs dominate.
- Phases 3–5 are independent of each other and of Phases 1–2; they can run
  in parallel with any of the above, staffed separately.
- Phase 6 (competitive benchmarking) should run last among the "assert A+"
  work — benchmarking against Slint/Flutter while Lumen is still mid-fix is
  not informative, and re-running it is cheap once instrumented.
- Track V (vello) has no hard dependency on anything above and should not
  block the A+ claim — it's an investment in the GPU backend's long-run
  maintainability, not a prerequisite for the numbers.

---

## What must be given up

No stated pillar is sacrificed by this path. What is genuinely spent:

- **Calendar time and headcount** — ~11–14 person-months is a substantial
  investment for a pre-1.0 project already carrying open work on the other
  four review axes (consumer API C+, modularity B-, resource usage C+/D,
  the missing `.lss` properties, mobile shell parity). Every month spent
  here is a month not spent there; the campaign's own "realistic outcome"
  section says as much explicitly.
- **A one-way protocol break** — CP6 requires the agent-facing `node-<N>`
  handle format to change (`nx-<hex>`, per the campaign's ID1/ID2), because
  a persistent `Tree` makes node-index recycling real for the first time.
  This is a deliberate, versioned, one-commit break (`lumen-semantics/1` →
  `/2`) with a migration window — not silent, but real: 12+ call sites
  across 5 crates, plus golden fixtures with literal `"node-11"` strings,
  need updating. Anyone who scripted against the old handle format has a
  migration to do.
- **Architectural surface area** — a persistent `LayoutTree` with recycling,
  `Rc<LayoutStyle>` at ~552 non-mechanical call sites (every
  `.style.field = …` becomes `Rc::make_mut(&mut el.style).field = …`), and
  the multi-`TaffyTree` split if Phase 2 is taken all add real complexity
  the current single-pass rebuild model doesn't have. This is the concrete
  form of "simplicity" being spent — not abandoned as a pillar, but taxed
  every time this code is touched going forward.
- **Real risk of a negative result at two points.** CP5 might legitimately
  say "stop" (if CP2.3 measures the taffy-node-mint cost at under 5%, the
  campaign's own plan says the retained-arena machinery isn't worth
  building) — in which case Phase 1 doesn't happen and the incremental-path
  ceiling stays at "ratio ≤ 1.0," not true O(changed), and A+ on this axis
  specifically is not reachable without revisiting that decision with new
  evidence. Similarly, the competitive benchmark (Phase 6) might come back
  unfavorable against Slint/Flutter on workloads Lumen hasn't been tuned
  for (deep trees, CJK-heavy text, sustained animation — all explicitly
  named as untested shapes in the perf review's benchmark critique). Both
  are real possible outcomes of following this path honestly, not
  hypothetical hedges.

---

## Confidence

**High confidence (directly grounded in source read or primary-source
research):**
- Every specific Lumen code citation (the `LayoutTree` wrapper, `ShapeKey`,
  `VirtualList`'s absolute-positioning implementation, `build_semantics`,
  `scene.rs`'s cull, the perceptual-tolerance constants, `rebuild_inner`'s
  fresh-`Tree`/fresh-`LayoutTree` swap, `IdHash = u128`) — read directly
  from the files cited.
- The N0 benchmark numbers and the CP-series diagnosis — the project's own
  falsifying measurement, cross-checked against source.
- The GPU-determinism research (vello's non-determinism bug in `#1323`, its
  CI using lavapipe, Skia Gold's per-config baselines, Flutter's documented
  fuzzy thresholds in `engine#40824`, Firefox's `fuzzy()`/`fuzzy-if()`) —
  fetched from primary sources (GitHub PRs/issues, project docs) directly,
  with quotes.
- **taffy's actual per-node incremental-layout cache** (`mark_dirty`'s
  early-stopping ancestor walk, `compute_cached_layout`'s cache-hit
  short-circuit, PR #246's 17s→3ms result) — verified against taffy's own
  source, not summarized from a description of it. This materially changed
  my initial (wrong) assumption that taffy was a monolithic whole-tree
  solve with zero partial-resolve capability; the corrected finding
  (Lumen never retains a tree long enough to use taffy's own cache) is
  now high-confidence, and it's a more optimistic finding for the layout
  blocker than the uncorrected version would have suggested.
- Flutter's `RenderObject.markNeedsLayout()`/relayout-boundary conditions —
  fetched from Flutter's own official API docs directly, with quotes.
- Slint's layout engine composition (custom `HorizontalLayout`/
  `VerticalLayout`/`GridLayout` predate and are independent of taffy;
  taffy powers only a newer, explicitly-experimental `FlexboxLayout`) —
  verified against Slint's actual `Cargo.toml` and its own 1.16 release
  blog, not inferred.
- Servo's rayon-based parallel layout (PR #34132, the wiki's fork-join
  description, and its explicit sequential fallback for cross-node
  dependencies like floats) and taffy's own open, unresolved,
  "controversial"-labeled multithreading issue (#27, since 2022) — both
  fetched directly from GitHub.
- Blink's `CachingWordShaper` word/CJK-character-granularity shaping cache
  and its two-level (shape-plan + shaped-result) design — quoted directly
  from Chromium's own text-stack README, not inferred from general
  knowledge of "browsers cache shaping."

**Medium confidence (corroborated by secondary sources, not primary-source
verified line-for-line):**
- Compose's `LayoutNode`/`measurePending` restart-scope model — sourced from
  official Android developer docs (high) plus community deep-dives on the
  exact mechanics (medium); the overall claim (Compose retains and
  incrementally invalidates a real layout tree) is solid, the precise
  internal naming is secondary-sourced.
- SwiftUI's Attribute Graph having a persistent, incrementally-cached layout
  step — explicitly **lower** confidence than the rest of this list; Apple
  does not publish these internals, so this rests on third-party
  reverse-engineering (aleahim.com, objc.io's Swift Talk), not primary
  documentation. Treat SwiftUI's inclusion in the "owns incremental layout"
  peer set as directionally likely, not verified.
- Dioxus's native/desktop renderer sitting on taffy (implying it shares
  Lumen's exact layout-incrementality profile, both the cache and the
  flexbox-coupling ceiling) — plausible and consistent with ecosystem
  knowledge, not independently re-verified against Dioxus's own source in
  this pass.

**Explicit estimates, not measurements:**
- All person-month figures in *The path*. These translate the project's own
  qualitative S/M/L/XL sizing into calendar time using my judgment; they are
  not derived from a work-breakdown or historical velocity data, because
  none exists in the repo for this class of work yet (the campaign's own
  M-C is the closest precedent, and it hasn't executed).
- The "11–14 person-months" total specifically should be read as an
  order-of-magnitude signal (this is a multi-quarter effort for a small
  team, not a multi-year one and not a multi-week one), not a committable
  schedule.

**Where I am most likely wrong:**
- The CP5 gate outcome (whether CP6 is worth building at all) — this is
  explicitly unmeasured in the repo today (CP2.3, the specific instrument
  that would decide it, hasn't run), so my treating CP6 as "probably worth
  it" is a judgment call the project's own plan correctly refuses to make
  in advance.
- vello's exact current maturity will have moved by the time this is acted
  on — the research pass captured a snapshot (GPU v0.9, Hybrid beta) that is
  explicitly time-sensitive; re-check before committing to Track V.
