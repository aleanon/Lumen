# Performance Review — Lumen GUI Framework

*Adversarial review, 2026-08-06. Scope: performance only. Read-only — no code
was modified, no `cargo build`/`test` was run; two narrow `cargo bench`
invocations' worth of criterion output already sitting in `target/criterion/`
(dated 2026-08-05, matching HEAD) was read directly instead of re-run, per the
disk-pressure constraint. All findings are grounded in `file:line` citations
against the code as it stands at commit `9d430ad`, cross-checked against the
project's own `docs/results-*.md` where they overlap.*

---

## Verdict

**Lumen is not currently peak performance, and the project's own instrumentation
proves it.** The architecture has real, well-built foundations — a genuine
struct-of-arrays tree, a fine-grained signal graph that is bench-verified to
re-run exactly the dirty scopes, an allocation-free identity/hashing scheme,
and a three-tier text-shaping cache that correctly avoids reshaping text that
hasn't changed. But the two mechanisms that would actually justify a
"peak-performance" claim — **O(changed) incremental rebuild** and
**damage-driven GPU rendering** — are both broken in the paths that matter
most for a real, interactive, GPU-windowed app, and this is not a reviewer's
inference: it is what Lumen's own 2026-08-05 `nodecost` bench suite measured
(`docs/results-node-cost-n0.md`) and what this review's file-level trace of
`present_to_surface` confirms independently. Concretely: the "incremental"
scoped-rebuild path is **1.44× slower and allocates 85% more** than doing a
full flat rebuild of the same tree (self-measured, `docs/results-node-cost-n0.md:45,56`,
confirmed at `crates/lumen-widgets/src/app.rs:2754-2850`); the damage rectangle
the renderer computes every frame is **discarded** on the live-window GPU
path, so every painted frame re-encodes the entire scene regardless of how
little changed (`crates/lumen-widgets/src/app.rs:4206-4209`,
`crates/lumen-render/src/gpu.rs:1027-1106`); and any CSS transition running
*anywhere* in the app disables the memoization system for the *whole* app for
its duration (`crates/lumen-widgets/src/app.rs:914,1144-1147`) — exactly when
the app is under the most per-frame pressure. On top of this, the one widget
pattern every real app needs for scale — a long list — has no shipped
virtualized implementation; the only documented scroll container
(`Scrollable`) lays out and paints every child every frame regardless of
viewport (`crates/lumen-widgets/src/scrollable.rs:1-3`). The project deserves
real credit for finding most of this itself, through an unusually rigorous,
self-falsifying benchmark culture (§ Benchmark critique) — that intellectual
honesty is a genuine asset most projects this size don't have. But credit for
diagnosis is not the same as the fix having landed, and as of this commit it
has not.

**Grade: D+.** Strong foundations, ambitious and well-reasoned design, and
real forward motion (the CP-series plan targets exactly the right things) —
but the specific mechanisms that would make "peak performance" true today do
not currently deliver it, by the project's own measurement.

---

## Scorecard

| Area | Rating | One-line justification |
|---|---|---|
| Core data structures (tree, signals, identity) | **Strong** | Genuine SoA tree with proptest coverage; fine-grained signal graph bench-verified to re-run exactly the dirty scope out of 10,000; allocation-free typed identity, 2.8× faster than string keys and proven so by a dedicated bench. |
| Incremental rebuild (`cx.scope`/memoization) | **Broken** | Self-measured 1.44× *slower* and 85% more allocation than a full rebuild for the framework's own literal acceptance-criterion shape (`docs/results-node-cost-n0.md`); `copy_node` mints a fresh tree node, fresh taffy node, and 4 HashMap remove+insert pairs per "memo hit" node. |
| Layout (taffy integration) | **Weak** | No persistent `TaffyTree`; every rebuild mints a brand-new arena and a new `NodeId` for every node, including copy-forward nodes; `Style→taffy::Style` is recomputed unconditionally, never cached; taffy's own `relayout_subtree`/`set_style` incrementality primitives exist in `lumen-layout` but are never called by the runtime. |
| Display-list emission | **Weak** | Full flat `Vec<DrawCmd>` rebuilt every frame (O(tree)); the one real incremental win (R5 glyph-run cache) is real and measured (~50×) but only covers glyph-run construction, not rects/gradients/images. |
| Damage tracking (paint) | **Broken** | The diff (`damage_between`) is computed correctly every frame, but on the live GPU-windowed path it is discarded — `present_to_surface` always re-encodes the whole scene; only the headless/CPU-readback branch actually consults the `Region`. |
| GPU submission | **Weak** | Real instanced-draw batching for consecutive same-type commands (good), but no persistent buffers/textures (everything is `create_buffer_init`'d fresh per frame), a hardcoded single 1024×1024 glyph-atlas page with clear-the-world eviction, and glyph runs never batch across runs — interleaved content (row background + label, the most common UI shape) roughly doubles draw-call count per row. |
| Text shaping (parley/swash) | **Adequate** | Zero-copy font loading is real; three-tier shape/run/glyph cache genuinely skips reshaping on unrelated changes and on recolor. But multi-line text editors reshape the *entire* document on every keystroke, and drag-select bypasses the cache entirely, calling uncached `layout()` on every pointer-move. |
| Observability (agent semantics tree) | **Weak** | Correctly O(depth) for event dispatch, but `build_semantics` — a full O(n) walk that clones every node's label/classes/actions/states and `format!`s a role string per node — reruns on *every* hover/focus/press restyle, not just structural rebuilds, directly contradicting the "cheap restyle, no rebuild" doc comment beside the call site. |
| List/large-content scaling | **Broken (as shipped)** | `vlist_1m_scroll` proves virtualization *works* (1.15 ms/frame) — but it is not a shipped widget. The only documented scroll container (`Scrollable`) is O(N) children every frame; real virtualization exists only as ~380 lines of unpackaged application code in one example. |
| Idle CPU / startup | **Adequate, honestly documented** | Idle-loop logic is genuinely correct (`ControlFlow::Wait`, one `about_to_wait` call in 12s); the residual idle CPU was root-caused (via a controlled Vulkan-ICD swap) to the NVIDIA driver, not Lumen — a rare, rigorous piece of self-correction. Startup is fully synchronous (font load + first layout/paint + GPU pipeline compile all serialized before the window is shown), and choosing the CPU renderer paradoxically still creates a second full GPU context. |
| Benchmark suite quality | **Adequate, but under-enforced** | The `nodecost.rs`/`identity.rs` instruments are genuinely rigorous (counting allocator, single-confounder isolation, self-falsifying design) — but only 5 of ~15 criterion benchmarks are wired into CI (`scripts/perf_gate.sh`); the benches that actually protect the incremental-path claims this review is most concerned about are not CI-gated at all. |
| Competitive benchmarking | **Weak** | The only external comparison is against GTK3/PyGObject (Python), explicitly caveated by the project's own doc as not evidence against real compiled competitors. No Slint, Flutter, egui, or GTK4 comparison exists anywhere in the repo, despite `plan-node-cost.md` explicitly invoking "Makepad's cost model" as the design's own motivating thesis. |

---

## Findings

### Critical

**F1. Damage is computed correctly, then discarded on the live GPU present path — every painted frame re-encodes the entire scene.**
*Evidence:* `crates/lumen-widgets/src/app.rs:4206-4209`:
```rust
if self.surface_attached {
    // Direct-to-surface (1c): no CPU rasterization. The shell presents the
    // retained `last_dl` via `present_to_surface` when `damage != None`
    // (granularity is ignored — the GPU renders the whole frame anyway).
} else {
    match damage { ... }  // only this (headless/CPU) branch actually uses the Region
}
```
`crates/lumen-render/src/gpu.rs:1027-1106` (`present_to_surface`) confirms: it calls `encode_root` with the full retained `DisplayList` unconditionally, which recursively creates a fresh `resolved`(+MSAA) texture per layer (`gpu.rs:1582-1614`) and a fresh vertex/index/instance buffer per batched draw-op (`gpu.rs:262-296,1358-1560`), then frees all of it (`drop(keep)`, `gpu.rs:1099`) right after submit.
*Why it costs performance:* on the path every real windowed app runs (`just run <example>` on this box's GPU), GPU submission scales with total scene size, not with damage size — the opposite of what "damage tracking" is supposed to buy. A single blinking caret or one hover-color change costs the same GPU work as a full-screen redraw.
*Fix:* in `present_to_surface`, call `list.culled_for_damage(dirty)` (already exists, already used by `Wgpu::render_damage`, `gpu.rs:328-350`) before `encode_root`, and clamp the render pass with `set_scissor_rect` to the dirty region. This is coupled to F5 below (persistent layer textures), since a scissored partial redraw needs `LoadOp::Load` rather than a fresh-cleared target every frame.

**F2. The "incremental" scoped-rebuild path (`cx.scope`/`copy_node`) is a measured pessimization, not an optimization.**
*Evidence:* `docs/results-node-cost-n0.md:45-56` (project's own falsification bench, run 2026-08-05):
> "Rebuilding all 500 rows is 1.44× faster than rebuilding one row and reusing 499 memoized subtrees... The 'incremental' path allocates 85% more than the full rebuild."

Confirmed directly in source: `crates/lumen-widgets/src/app.rs:2754-2757` (the `copy_span` filter, O(scopes² × span)):
```rust
let nested: Vec<(IdHash, SpanRec)> = self.prev_spans.iter()
    .filter(|(k, r)| **k != key && prev_nodes.contains(&r.root))
```
and `copy_node` (`app.rs:2789-2850`) — for *every* node in a memoized ("unchanged") subtree — calls `tree.insert_root()`/`insert_child()` (a brand-new tree node), 4 separate `HashMap::remove`+`insert` pairs, a `LayoutStyle::clone()` (`app.rs:2841`), and mints a fresh taffy leaf/container node via `layout.leaf`/`layout.container`.
*Why it costs performance:* this is the framework's own literal acceptance-criterion shape (500-row list, 1 row dirty) — the exact pattern the F-series documentation tells app authors to write for performance. As measured, writing that code makes the frame slower, not faster, and reproducibly so (bench artifacts confirmed fresh at `target/criterion/text_list_scoped_changed_frame/` — 1,120,801 ns vs `text_list_changed_frame`'s 777,193 ns, matching the doc's 1,114.0 µs / 776.17 µs to within noise).
*Fix:* already scoped by the project (`docs/plan-incremental-path.md` CP1/CP2): give `copy_node` a path that reuses the previous taffy `NodeId` in place via `LayoutTree::set_style` (only marking dirty when the style differs) instead of minting new nodes; this requires a persistent `LayoutTree` across rebuilds (see F4).

**F3. Any CSS transition or keyframe animation running anywhere in the app disables incremental memoization for the entire app, for its duration.**
*Evidence:* `crates/lumen-widgets/src/app.rs:914`:
```rust
self.allow_copy_forward = !visual_changed && !anims_running && !full_rebuild_forced();
```
`anims_active()` (`app.rs:1144-1147`) is `true` while *any* `prop_anims`/`key_anims` entry anywhere in the tree is mid-flight. `build_node` (`app.rs:3138`) gates the entire `copy_span` fast path on this single app-wide boolean.
*Why it costs performance:* a single button's hover-color fade, or a toast's entrance transition, forces every `cx.scope`/`keyed()` region in the whole app — including an unrelated 1,000-item list elsewhere on the same screen — to fully re-materialize and re-lower from scratch, every frame, for the transition's duration (and `is_time_driven()` keeps the shell pumping at up to 60 fps while this holds, `app.rs:1393-1397`). This means the memoization system turns itself off exactly when the app is under the most sustained per-frame pressure it will ever see.
*Fix:* scope the suppression to the animating node's ancestor chain / affected spans, not the whole app — the code already tracks per-scope `span_ctx_hash`, which is the natural place to add "does this span contain an active animation."

**F4. No shipped list virtualization — the only documented scroll container is O(N) every frame.**
*Evidence:* `crates/lumen-widgets/src/scrollable.rs:1-3`, in the widget's own doc comment:
> "(For very long lists, virtualize — this lays out all children.)"

`Scrollable::new` (`scrollable.rs:41-107`) builds `Element::column(children)` from the *entire* `children` vector unconditionally; `clip: true` hides the overflow visually but taffy still lays out, and the renderer still paints, every off-screen row every rebuild. A real windowed list *does* exist and *does* work — `vlist_1m_scroll` measures 1.15 ms/frame — but only as `widgets::VirtualList`/`virtual_list` (`crates/lumen-widgets/src/widgets_m1.rs:543-623`, confirmed present and correctly windowing by scroll offset + overscan) *or*, for anything richer, as ~380 lines of hand-rolled, unpackaged windowing logic in `examples/catalog/src/lib.rs`.
*Why it costs performance:* `VirtualList` exists but is easy to miss — the natural, discoverable API for "a list of things" is `Scrollable` (or a plain `column` inside one), and both are O(N). Any real app with a list of hundreds of rows that reaches for the obvious API pays an O(N) layout+paint tax every frame, exactly the class of workload `comparison-gtk-mintupdate.md` identifies as GTK's structural advantage (`GtkTreeView` cannot accidentally render 500 rows; Lumen currently can, by default).
*Fix:* this is a documentation/discoverability problem more than an engineering one — `VirtualList` already exists and works. Promote it in the widget catalog and `building-apps` skill guidance as the default for any list past ~50-100 items; consider a runtime diagnostic warning when `Scrollable`/`column` exceeds a child-count threshold.

### High

**F5. No persistent GPU buffers or textures — every frame allocates and frees dozens of GPU resources, and the gradient ramp is uncached unlike every other cacheable resource.**
*Evidence:* `crates/lumen-render/src/gpu.rs:1139-1152` (viewport uniform+bind group, recreated every `encode_root`), `1358-1366`/`1395-1399`/`1444-1456`/`1556-1560` (composite/gradient/image/glyph instance buffers, each `create_buffer_init`'d fresh per batched run), `1582-1614` (fresh `resolved`(+MSAA) texture per `PushLayer`, per frame). `upload_ramp` (`gpu.rs:1831-1877`) has **no cache** — contrast `upload_image` (`gpu.rs:2049-2127`, content-hash cached with half-retention eviction) and `PathGeometry::add_cached` (`gpu.rs:2380-2420`, persistent `tess_cache`).
*Why it costs performance:* driver-level buffer/texture allocation is not free; destroying and recreating same-shaped resources every frame instead of reusing them is a textbook anti-pattern the CPU-readback presenter one layer up already avoids (`crates/lumen-shell/src/lib.rs:1546-1547`, explicit "reuse across same-size frames" comment) but the GPU path doesn't apply to itself. Every progress bar, badge, or gradient button repays a full ramp-texture upload every frame it's visible.
*Fix:* pool per-layer render targets keyed by (layer identity, size); reuse instance buffers via a ring buffer or `queue.write_buffer` into persistently-sized buffers instead of `create_buffer_init`; add a `(stops-hash) → bind group` cache mirroring `img_cache`.

**F6. No persistent taffy tree — every structural rebuild mints an entirely new `TaffyTree`, new `NodeId`s for every node, and unconditionally recomputes `Style→taffy::Style`.**
*Evidence:* `crates/lumen-layout/src/tree.rs:32-38` (`LayoutTree::new()` constructs a fresh `taffy::TaffyTree` and empty `abs` map), called fresh inside `rebuild_inner` on every `rebuild()` (`crates/lumen-widgets/src/app.rs:2579-2580`). Every node — freshly built *or* copy-forwarded — calls `layout.leaf`/`layout.container` (`tree.rs:42,50`), which calls `taffy.new_leaf`/`new_with_children`, minting a brand-new `NodeId` every time; `LayoutTree::set_style` (`tree.rs:56-60`, the actual "patch one node" primitive) has **zero call sites** in `lumen-widgets`. `LayoutStyle::to_taffy()` (`crates/lumen-layout/src/style.rs:267`) is invoked unconditionally per node per rebuild with no memoization, even when the `LayoutStyle` is byte-identical to the previous frame.
*Why it costs performance:* every structural signal write is O(total node count) in layout, not O(dirty subtree) — this is true regardless of whether 1 node or 500 changed, and it's the mechanism underlying F2's measured 1.44× regression (the fresh-taffy-node-per-copied-node cost is one of the three root causes the project's own `plan-incremental-path.md:98-100` names).
*Fix:* make `LayoutTree` persistent across rebuilds; have `copy_node` call `set_style` (only when the style actually differs) on the *existing* taffy `NodeId` instead of minting a new one. Note taffy's own `relayout_subtree` (`tree.rs:80-92`) is *not* a safe drop-in — the project's own docs (`docs/plan-incremental-path.md:266`) record it as "pins the subtree to its existing box and never propagates a size delta — wiring it in as-is is a layout-corruption bug."

**F7. Display list is a full, flat rebuild every frame — O(tree) emission with no retained/patched structure.**
*Evidence:* `crates/lumen-widgets/src/app.rs:3556-3562` (`build_display_list`): `DisplayList::new()` allocates a fresh empty `Vec<DrawCmd>` (plus fresh `images`/`runs`/`glyph_images` vectors), then walks `self.tree.document_order()` — itself a fresh `Vec<NodeIndex>` allocated every call (`crates/lumen-core/src/tree.rs:231-236`) — emitting every node's commands. `damage_between` (F1) is a *post-hoc diff* of two such full lists, not evidence of incremental construction.
*Why it costs performance:* per the project's own R5 profiling (`docs/plan-rendering-performance.md:337-386`), `build_display_list` is "the dominant remaining per-frame cost on a changed frame" — 15.1 ms for 500 nodes before the glyph-run-cache slice landed, now 304 µs *for text-run construction specifically*, but rect/gradient/image emission still walks the whole tree unmemoized.
*Fix:* the project's own R5.1–R5.3 (per-subtree display-list fragment caching, splice by scope-skip + origin-shift) is the designed fix and is not yet built. Note this is coupled to F1 — even a perfectly incremental display list doesn't help the live GPU path until F1 lands, since `present_to_surface` ignores the diff regardless of how cheaply it was computed.

**F8. The CPU "golden" damage path never actually reduces raster work, and silently degrades to full-frame at any HiDPI scale ≠ 1.0.**
*Evidence:* `crates/lumen-render/src/cpu.rs:52-77` (doc comment + `render_damage`): the CPU backend always rasterizes the *full* frame (`render(list, width, height, background)`) and only crops the result — this is documented and deliberate (tiny-skia's AA coverage is not translation-invariant, so cropping a full-space render is the only way to stay byte-identical for the golden-correctness contract). Separately, `crates/lumen-render/src/lib.rs:156-178` (`TinySkia::render_damage`) only calls the culled path when `scale == 1.0`; at any other scale (i.e. essentially every HiDPI display) it falls back to a full `render_frame` + crop with **no command culling at all**.
*Why it costs performance:* anywhere the CPU backend is active (the default renderer, and always for headless/golden/agent paths), "damage tracking" saves only the final blit-rect cost, not the raster work that the project's own numbers show dominates the frame budget (23.4 ms CPU raster for a 500-node frame, per `docs/plan-rendering-performance.md`).
*Fix:* out of scope for a byte-identical CPU golden per the code's own stated design; the correct fix is display-list-level incrementality (F7), not pixel-level. Worth flagging because "damage tracking landed" reads, from the docs alone, as if CPU raster work scales with damage — it does not, ever, at HiDPI.

**F9. GlyphRun draw calls never batch across runs, and any interleaving with Rect/Path/Image forces a flush — realistic content roughly doubles draw-call count per row.**
*Evidence:* `crates/lumen-render/src/gpu.rs:1497-1568` (`DrawCmd::GlyphRun` handling): every glyph run calls `flush_rects`/`flush_paths` first (line 1499-1500) and then pushes its own `LayerDraw::Glyphs` op — there is no `pend_glyphs` accumulator analogous to `pend_rects` (`gpu.rs:257-272`, which *does* batch consecutive same-type rects into one instanced draw call). So two adjacent text runs are never merged into one draw call, and a typical list row shaped as `[Rect(background), GlyphRun(label)]` forces a flush at every type transition.
*Why it costs performance:* for N rows of `[background rect, text label]` — the most common list/table/form row shape — this produces close to 2N draw calls instead of the 1-2 an optimal batcher would emit (one rect-instance draw + one glyph-instance draw for the whole visible set). Draw-call count, not raw fragment cost, is usually the actual GPU-submission bottleneck for UI-shaped content.
*Fix:* add a `pend_glyphs: Vec<GlyphInstance>` accumulator mirroring `pend_rects`, flushed only when interrupted by a non-glyph command or a different atlas page, so adjacent text runs merge into one draw call the same way adjacent rects already do.

**F10. `flush()`'s dirty-scope queue is O(n²) in fan-out width.**
*Evidence:* `crates/lumen-core/src/state.rs:1025-1038`:
```rust
let id = b.dirty.remove(0);   // O(n) shift, every pop, on a Vec<ScopeId>
```
*Why it costs performance:* a single write to a widely-subscribed signal (a theme signal, a window-size signal, a shared filter — exactly the signals many widgets read) with *k* dependent scopes costs O(k²), not O(k), because every pop of the FIFO shifts the remaining elements. No existing bench dirties more than a handful of scopes at once, so this gap is unmeasured, not just unfixed.
*Fix:* swap `Vec<ScopeId>` for `VecDeque<ScopeId>` and `pop_front()`, or drain by swap-remove since strict FIFO order isn't semantically required.

**F11. The agent-observability semantics tree is fully rebuilt — O(n) walk + per-node clones + `format!` — on every hover/focus/press restyle, not just structural rebuilds.**
*Evidence:* `crates/lumen-widgets/src/app.rs:2229`, inside `restyle_visual` (the path whose own doc comment at `app.rs:2158-2162` claims *"R2 damage limits the raster to exactly the changed region — no rebuild, no relayout, no scope re-run"*):
```rust
self.sem_root = Some(self.build_semantics(self.tree.root()));
```
`build_semantics` (`app.rs:4277-4324`) recurses the whole tree and, per node, does `format!("{:?}", m.role)` plus `.clone()` on label/classes/actions/states.
*Why it costs performance:* the cheapest possible interaction in the framework — a mouse hovering over a new widget, with no signal write and no layout change — still pays a full-tree observability rebuild. This directly undercuts one of Lumen's two stated design goals (complete AI-agent observability) by making it a hidden per-interaction tax rather than an on-demand query, and it is unbenchmarked — no `benches/*.rs` file measures `build_semantics` at all.
*Fix:* give `SemanticsNode` construction the same reachability-based incrementality the paint path has — either skip it entirely on `restyle_visual` and patch just the flipped node's `states` in place in the existing `sem_root`, or memoize per-node semantics the same way `scope_cache` memoizes view output.

**F12. Multi-line text editors reshape the entire document buffer on every keystroke.**
*Evidence:* `crates/lumen-widgets/src/text_field.rs:78-82` and `crates/lumen-widgets/src/widgets_m4.rs:561-564` both re-stringify the *whole* editor buffer into one text node on every edit (`editor.get(rt).text().to_string()`). Because `ShapeKey` includes the full `text: String` (`crates/lumen-text/src/lib.rs:207`), any single-character edit anywhere in the buffer is a guaranteed cache miss, and parley reshapes the entire document from scratch.
*Why it costs performance:* for a short single-line input this is free; for `TextField`/`RichTextEditor` on a nontrivial document, every keystroke costs O(document length) shaping work, and for `RichTextEditor` specifically this compounds with a full markdown-lite re-parse of the whole source on every keystroke too (`widgets_m4.rs:576`).
*Fix:* shape per-line (split on `\n`, key the cache per line + line-start offset) so an edit on line N only invalidates that line's `ShapeKey`; keep whole-buffer shaping only for genuinely single-line inputs.

**F13. Drag-select in text editors bypasses the shape cache entirely, firing an uncached full-document reshape on every pointer-move.**
*Evidence:* `crates/lumen-widgets/src/app.rs:2037-2082` (`place_caret`, `move_caret_vertical`) call `self.text.layout(...)` directly — the raw, uncached parley entry point — not `self.text.shaped(...)`.
*Why it costs performance:* a mouse drag-select gesture fires pointer-move at up to 60-120 Hz; combined with F12's full-buffer shaping cost, dragging a selection across a large document reshapes the whole thing dozens of times per second, with zero cache benefit even though the exact same text was almost certainly already shaped this frame for painting.
*Fix:* route both call sites through `self.text.shaped(...)` instead of `.layout(...)` — the cache key will typically already be warm from the same-frame paint call.

**F14. Only 5 of ~15 criterion benchmarks are wired into CI — the ones that actually protect the incremental-path claims are not among them.**
*Evidence:* `scripts/perf_gate.sh:12-32` checks exactly five absolute-nanosecond budgets (`layout_10k_dirty_subtree`, `vlist_1m_scroll`, `data_grid_1m_scroll`, `cull_100k`, `idle_frame`); `.github/workflows/ci.yml:61` is the only CI wiring, and it invokes only `perf_gate.sh` — `cargo bench --bench nodecost` and `--bench identity` are never run in CI.
*Why it costs performance (indirectly, but critically):* `nodecost.rs` and `identity.rs` are the instruments that produced F2's headline finding and the O(scopes²) result in `docs/results-node-cost-n0.md §3` — exactly the benchmarks a project in the middle of fixing a self-diagnosed regression most needs protected. As it stands, any future change can silently make F2/F3 worse (or silently "fix" them without evidence) with no CI signal either way.
*Fix:* the project's own `docs/plan-incremental-path.md` CP0 already scopes this (ratio-based gates, noise-aware thresholds, machine-readable baselines) — it has not landed yet.

### Medium

**F15. The GPU glyph atlas is hardcoded to a single 1024×1024 page with no LRU eviction — on overflow, the whole atlas is wiped and every glyph on screen thrashes.**
*Evidence:* `crates/lumen-render/src/gpu.rs:897`: `GlyphAtlas::new(ATLAS_SIZE, 1)` — `max_pages` is hardcoded to `1` even though the packer supports more (`crates/lumen-render/src/atlas.rs:37-80`). Eviction (`atlas.rs:154-159`, `clear()`) drops *every* packed glyph and page — there is no partial/age-based eviction. On overflow (`gpu.rs:1509-1513`, `atlas_overflow.set(true)`), the next frame calls `self.atlas.borrow_mut().clear()` (`gpu.rs:1101-1104`) unconditionally.
*Why it costs performance:* a text-heavy screen — a large character set, many distinct font-size/weight combinations, or a long scrolling document with a large unique-glyph working set — can realistically exceed one 1024² R8 page. Once it does, every subsequent frame's glyphs are "fresh" again (full re-rasterize + re-upload), and if the working set doesn't shrink, this repeats every frame: a genuine thrashing cliff, and it hits exactly the workload class (data grids, long lists, log/code viewers) the project's own `vlist_1m_scroll`/`data_grid_1m_scroll` benches are meant to represent as strengths.
*Fix:* raise `max_pages` above 1 (the allocator already supports it), and/or add real LRU eviction (age/last-use per slot) instead of clear-the-world.

**F16. `LayoutStyle` is cloned once per node per rebuild, on both the fresh-build and the copy-forward paths.**
*Evidence:* `crates/lumen-widgets/src/app.rs:3491` (fresh build) and `:2841` (copy-forward) both do `self.node_layout_style.insert(node, style.clone())` / `.insert(node, lstyle.clone())` immediately before handing the original to `layout.leaf`/`layout.container`.
*Why it costs performance:* even a scope that hit the memo cache and skipped its view closure still pays a full `LayoutStyle` clone plus a fresh `to_taffy()` conversion (F6) plus a new taffy node — this is one of the concretely-named contributors to F2's 85%-more-allocation result.
*Fix:* coupled to F6 — once `LayoutStyle` identity is tracked per persistent taffy node, a copy-forward with an unchanged style needs no clone at all.

**F17. `Signal::set` always heap-allocates via `Box::new`, even for primitive types.**
*Evidence:* `crates/lumen-core/src/state.rs:957-963`: `slot.value = Box::new(value)` on every `.set()`, because the store is type-erased (`Box<dyn StoredValue>`). `Signal::update` avoids this by mutating in place via `downcast_mut` (`state.rs:1090-1120`), but that's opt-in.
*Why it costs performance:* `set()` is the more obvious/default API; every call — `Signal<bool>`, `Signal<i32>`, anything — pays a malloc/free pair that `update()` would avoid. This is an inherent cost of the type-erased store, not a bug, but "fine-grained reactivity" controls *who* reruns, not the cost of storing the new value, and that distinction isn't visible to an app author choosing between `set`/`update`.
*Fix:* document `update()` as the default recommendation even for scalar types where the closure is trivial (`|v| *v = new`), or add a specialized non-erased fast path for `Copy` types.

**F18. `scene::cull_visible` (viewport culling) exists but is dead code — nothing in either render backend visibility-culls the display list.**
*Evidence:* `crates/lumen-render/src/scene.rs:29-70` implements a `std::thread::scope`-based parallel viewport cull; its only caller in the whole workspace is its own unit test (`crates/lumen-render/tests/scene.rs:3`). Neither `cpu.rs`'s `Renderer::run` nor `gpu.rs`'s `encode_layer` culls against a viewport before processing.
*Why it costs performance:* every `DrawCmd` in the built list is processed by both backends regardless of actual visibility — content scrolled off-screen inside a clip layer still gets fully encoded/rasterized, relying entirely on the clip mask to hide it visually. GPU/CPU submission cost scales with total painted scene size, not visible scene size, compounding F1/F7/F8.
*Fix:* wire `cull_visible` into `encode_layer`/`Renderer::run` (cheap, since `paint_bounds()` already exists per-command), or remove/clearly mark it experimental so it doesn't read as already integrated.

**F19. `ThreadPoolSpawner::default()` spawns `available_parallelism()` threads unconditionally, even for apps that never spawn a task.**
*Evidence:* `crates/lumen-core/src/tasks.rs:274-282`. Independently confirmed via the project's own investigation (`docs/results-idle-and-gpu-context.md:104-109`): 32 threads for a trivial `counter-win` app that never runs a task.
*Why it costs performance:* the threads park on a channel and cost no CPU at idle, but they cost stacks, scheduler entries, and roughly 900 MB of `VmSize` — a cost that matters far more on a phone than on this dev box, and mobile is a first-class target per the project's own stated goals.
*Fix:* lazily grow the pool, or size it to `min(4, cpus)` until the first `spawn`.

**F20. Selecting the CPU renderer doesn't avoid the GPU context — it creates a second one, purely to blit.**
*Evidence:* `crates/lumen-shell/src/lib.rs:491-497`:
```rust
self.direct = headless.attach_surface(window.clone().into(), ...);
self.presenter = if self.direct { None } else { Some(Presenter::new(window.clone())) };
```
`Presenter::new` (`lib.rs:1457`) builds its own full `wgpu::Instance`/adapter/device stack. `LUMEN_RENDERER=cpu` correctly selects `TinySkia` for rasterization but makes `attach_surface` report it cannot present directly, which *triggers* this second wgpu context purely to blit CPU pixels to the window.
*Why it costs performance:* the branch is inverted from user intent — asking for the CPU renderer, presumably to save the GPU/driver residency cost, guarantees a wgpu context exists anyway. Measured by the project itself: ~123 MB of driver/shader-compiler residency that a genuinely CPU-only app (e.g. GTK+cairo) never pays (`docs/results-idle-and-gpu-context.md §2.2`).
*Fix:* already scoped by the project as an ADR-003 escalation for a `softbuffer`-backed `SoftPresenter` (`docs/results-idle-and-gpu-context.md §2.4`) — correctly identified, not yet built.

**F21. Every interactive widget re-allocates its `Rc<dyn Fn>` event handlers on every non-memoized rebuild.**
*Evidence:* `Element`'s handler fields (`crates/lumen-widgets/src/element.rs:21,34,39,41,44,51`) are `Rc<dyn Fn(...)>`, and every widget constructor allocates them fresh via `Rc::new(move |...| ...)` (confirmed across ~25 widget files, e.g. `widgets_m1.rs:289`).
*Why it costs performance:* unless a widget sits inside a `cx.scope`/`keyed()` region that hits the memo cache, every rebuild reallocates every closure for every interactive widget — continuous heap churn during a drag, a text edit, or any animation that forces a rebuild (compounded by F3, which makes animation the exact case where memoization is off).
*Fix:* encourage `cx.scope` wrapping for widget-heavy static regions by default; this is a harder structural fix without more infrastructure for recognizing unchanged closure captures.

**F22. `svg::render` has no cache, and the official example calls it inline inside a `view`/`build` closure.**
*Evidence:* `crates/lumen-render/src/svg.rs` (stateless parse+rasterize, correctly no internal cache); `examples/svg/src/lib.rs:31` calls `lumen_render::svg::render(...)` directly inside `fn build(cx: &mut BuildCx) -> Element` — a closure that reruns on every rebuild of that screen.
*Why it costs performance:* any app author copying this official example pattern re-parses and re-rasterizes SVG icons from scratch on every state change anywhere on that screen, unless F1-unrelated scope memoization happens to skip the closure entirely.
*Fix:* not a crate bug — a documentation/lint gap. Note in `building-apps`/`styling-lss` guidance that `svg::render` output must be computed once (behind a signal/memo, or at startup) and reused.

**F23. Startup is fully synchronous and single-threaded — the window is shown before font registration, first layout/paint, and GPU pipeline compilation complete.**
*Evidence:* `crates/lumen-shell/src/lib.rs:455-524` (`resumed`): `window.set_visible(true)` happens before `app.run_headless(...)` (which registers every embedded font and does a full synchronous rebuild+layout+paint) and before `Presenter::new`/`attach_surface` (which `block_on`s GPU adapter/device negotiation and compiles the blit shader).
*Why it costs performance:* first-frame latency scales linearly with (font count × parse cost) + (root tree size × layout/paint) + (GPU adapter negotiation + shader compile), all serialized on one thread with no placeholder frame. Fine for the small examples measured so far (250 ms cold start, `docs/comparison-gtk-mintupdate.md §5`), but a real ceiling once an app has a nontrivial first screen or multiple embedded fonts.
*Fix:* show the window only after the first paint completes (or show a placeholder first); parallelize font registration with GPU device/pipeline setup.

**F24. `ShapeKey::new` allocates a `String` clone of the full text on every `shaped()`/`shaped_run()` call, even on a cache hit.**
*Evidence:* `crates/lumen-text/src/lib.rs:219-230`: `text: text.to_string()` runs unconditionally before the `HashMap` lookup that would otherwise discard the key on a hit.
*Why it costs performance:* called for every visible text node in both the measure pass and the paint pass, every rebuild — a screen with hundreds of text nodes allocates hundreds of throwaway `String`s per frame purely to perform a lookup.
*Fix:* use a borrow-friendly lookup (e.g. a raw-entry API keyed on `(&str, style-bits)`) that only allocates on an actual insert.

**F25. `intern_glyph_ref` re-clones cached glyph bitmap bytes into a fresh `Vec` every frame, with an O(n) linear scan.**
*Evidence:* `crates/lumen-render/src/display_list.rs:411-418`:
```rust
if let Some(i) = self.glyph_images.iter().position(|g| g.key == img.key) {
    return i as u32;
}
...
self.glyph_images.push(img.clone());   // clones the coverage Vec<u8> bitmap
```
Because `DisplayList` is rebuilt fresh every frame (F7), `glyph_images` starts empty every frame, so every distinct on-screen glyph's rasterized bitmap is re-cloned (heap alloc + memcpy) every frame even though swash-side rasterization is already cached (`GLYPH_CACHE`, `lumen-text/src/lib.rs:102-107`). The linear `.position()` scan against everything accumulated so far this frame also makes this O(k²) in distinct on-screen glyphs.
*Why it costs performance:* the CPU-side "don't re-touch pixel data for an unchanged glyph" goal is only half-achieved — rasterization is cached, but the byte copy into the display list isn't.
*Fix:* key `glyph_images` by `HashMap<u64, u32>` instead of linear scan; longer-term, have the atlas track cross-frame residency so this step can be skipped when the atlas already holds the glyph.

**F26. `NodeFlags::DIRTY_LAYOUT`/`DIRTY_PAINT` are dead fields that oversell the design.**
*Evidence:* `crates/lumen-core/src/tree.rs:29-32` declares and documents these as per-node staleness flags ("Layout of this subtree is stale" / "Paint of this node is stale"). A repo-wide grep confirms neither flag is ever set or read anywhere in the codebase.
*Why it costs performance (as a review-integrity issue, not a runtime one):* this is the API surface that would make `tree.rs`'s own doc comment ("damage aggregation are linear scans/walks over these arrays") true, and it doesn't exist — the real damage mechanism lives entirely in `lumen-render` (F1/F7), operating on display-list diffs, not per-node tree flags. An auditor (or a future contributor) reading `tree.rs` alone would reasonably conclude lumen-core does per-node dirty propagation; it does not.
*Fix:* delete the dead fields, or wire them into the actual invalidation path and make `tree.rs`'s doc comment accurate.

**F27. The canvas `fill_text` path has its own third, inconsistent text cache that fully clears (not half-retention) on overflow.**
*Evidence:* `crates/lumen-widgets/src/app.rs:4117-4151` — a bespoke string-keyed image cache for canvas text, capped at 512 entries, that clears entirely when the cap is crossed, unlike the half-retention eviction policy used everywhere else in the text stack (`lumen-text/src/lib.rs`'s `SHAPE_CACHE_CAP`/`RUN_CACHE_CAP`/`GLYPH_CACHE_CAP`).
*Why it costs performance:* a canvas-heavy screen (charts, custom-drawn widgets) that crosses 512 distinct text draws thrashes its entire text-image cache at once, rather than degrading gracefully.
*Fix:* route canvas text through `TextEngine::shaped`/`shaped_run` and apply the same half-retention eviction used elsewhere, rather than maintaining a third caching scheme.

### Low

**F28. `lint()`'s tofu-glyph diagnostic bypasses the shape cache** — calls `self.text.layout(...)` directly (`app.rs:1481`) to compute `.missing_glyphs()`, redundantly re-shaping text already shaped this frame for painting. Low impact: only reachable via the diagnostics/agent surface, not a per-frame call.

**F29. `layout_ellipsized` is O(n²) uncached shaping per character** (`crates/lumen-text/src/lib.rs:536-558`) — currently dead code (only called from a test), but it is public API documented as the `text-overflow: ellipsis` primitive, and whoever wires up CSS `text-overflow` will reach for it as-is. Fix before it's wired up: binary-search the truncation point via `measure_prefix` instead of a linear per-character scan with a `format!()` allocation each iteration.

**F30. `InlineSpawner::spawn` blocks the calling thread until the future resolves** (`crates/lumen-core/src/tasks.rs:183-186`), documented as test/deterministic-only, with the production default correctly being `ThreadPoolSpawner`. The type system doesn't prevent a misconfigured host from leaving `InlineSpawner` wired into production, where it would silently stall the UI thread on any await.

**F31. Every raw `CursorMoved` event triggers a full `pump()` before damage is known** (`crates/lumen-shell/src/lib.rs:1074-1081`, `inject()` calls `redraw_all()` unconditionally). GPU presentation is still correctly damage-gated downstream, so this doesn't cost GPU work, but every mouse-move message costs a full event-routing + hit-test pass regardless of whether it crosses any hoverable widget boundary.

---

## Unsupported claims

- **`.ai_docs/01-architecture.md:67`, "120 fps capable desktop; 60 fps floor mid-range mobile"** — not gated in CI, and not substantiated by any sustained-fps/frame-time-percentile bench anywhere in `benches/`. Given F1-F3 (GPU damage discarded, memoization a pessimization, animation disables memoization app-wide), this claim is actively contradicted for any screen with a running transition, which is exactly the "capable desktop" case a reader would assume the claim covers.
- **`tree.rs:6`, "damage aggregation are linear scans/walks over these arrays"** — false as written for `lumen-core`; no live damage mechanism exists in that crate (F26). The real damage system is in `lumen-render` and operates on display-list diffs, not on the tree arrays this comment describes.
- **`app.rs:2158-2162`, restyle_visual's doc comment: "R2 damage limits the raster to exactly the changed region — no rebuild, no relayout, no scope re-run"** — true for layout/relayout, false for the semantics tree it rebuilds two lines later (F11).
- **`docs/plan-node-cost.md`'s invocation of "Makepad's cost model"** as the design's motivating thesis (referenced but never benchmarked) — no Makepad, Slint, egui, or Flutter comparison exists anywhere in the repository. The only external comparison is against GTK3/PyGObject, which the project's own doc explicitly disclaims as not evidence against real compiled competitors (`docs/comparison-gtk-mintupdate.md §8`).
- **`.ai_docs/06-task-graph.md:167-168`, M6-exit "holds 120 fps desktop / 60 fps mobile and passes every perf gate"** — the task graph's own parenthetical concedes this was "wall-clocked around pump (`app.perf` is stubbed); no mobile legs" — i.e. measured with a stub instrument and never run on mobile hardware. The mobile extrapolation that does exist (`docs/results-node-cost-n0.md §7`) is explicitly labeled a *floor*, not an estimate, from an x86_64-under-KVM emulator, and puts a 500-node scoped frame at 30-84% of frame budget on a plausible mid-range phone — the opposite of "holds."
- **The project's identity-fold/hash claims** (`state.rs:217,570`, "never allocates," "costs no allocation to re-address") are, unusually, *well* supported — bench- and test-verified (`benches/benches/identity.rs`, `state.rs:1397-1421`). Flagging this here only to note the contrast: this project's claims are not uniformly unsupported, just unevenly so, and the gap correlates almost exactly with which claims have a dedicated falsifying bench and which don't.

---

## Benchmark critique

**What the suite gets right.** `benches/benches/nodecost.rs` and `identity.rs` are genuinely rigorous: a custom allocation-counting `GlobalAlloc` wrapper turns "is this allocation-bound?" into arithmetic rather than inference (`nodecost.rs:39-72`); each bench varies exactly one confounder (`scope_scaling` holds total node count at 600 and varies only scope count; `text_vs_rect_frame` holds tree shape and signal traffic identical and varies only leaf content); explicit warm-up passes separate cold-cache/first-build cost from steady-state cost. This is the instrument that produced F2 and the O(scopes²) result — the project used its own benchmark suite to falsify its own architecture's central claim, and documented the result plainly (`docs/results-node-cost-n0.md:1`, "the node-cost thesis is falsified"). That is a rare, valuable degree of intellectual honesty and should be recognized as such, separate from the fact that the underlying problem is unfixed.

**What it doesn't prove.** `layout_10k_dirty_subtree` (CI-gated, `perf_gate.sh`) calls `tree.relayout_subtree` directly on a pre-built taffy tree, bypassing `app.rs`/the real reactive pump entirely — and the project's own docs (`docs/plan-incremental-path.md:266`) record that `relayout_subtree` "pins the subtree to its existing box and never propagates a size delta — wiring it in as-is is a layout-corruption bug," i.e. this CI-gated bench measures a code path the project itself says is not safe to use as-is in the live pump. A green `layout_10k_dirty_subtree` gate currently proves nothing about real incremental-layout cost.

**Coverage gap.** As detailed in F14, only 5 of ~15 criterion targets are CI-gated, and none of the five is `text_list_scoped_changed_frame`, `scope_memo_one_of_many`, `scope_scaling`, or `allocs_per_frame` — the benchmarks that actually exercise the incremental-rebuild claim this review is most concerned about. `identity.rs`'s `assert_eq!(by_typed, 0, ...)` is a genuinely good pattern (a hard, CI-breaking allocation assertion rather than an observational report) — but it only fires if someone manually runs `cargo bench --bench identity`; it is invisible to CI.

**The GTK comparison is honest about its own limits, more so than most vendor comparisons.** `docs/comparison-gtk-mintupdate.md` uses matched workload (500-row list, same operations), same hardware, and explicitly separates the "ratio" claim (architecturally meaningful, language-independent: GTK's incremental/full ratio is 0.003, Lumen's is 1.44) from the "absolute" claim (mostly a Python-vs-Rust language-tier gap, explicitly flagged as such, §2). It also explicitly disclaims the comparison as evidence of "peak performance" against real competitors (§8). The risk is entirely that a reader skims the headline table (776 µs vs 10,190 µs) without the surrounding prose — the tables read stronger than the caveats fully prevent.

**What's missing entirely:**
- **No animation/frame-time-percentile bench.** Nothing measures p50/p95/p99 frame time or jank under sustained `Poll`-mode redraw (e.g. a running transition), despite F3 making this precisely the case where the framework's own memoization turns itself off.
- **No windowed/GPU-presented scroll-fps bench.** `vlist_1m_scroll`/`data_grid_1m_scroll` are headless CPU-pump benches; nothing measures real GPU-presented scroll performance despite a live GPU and a live-window agent both being available in this environment.
- **No memory-growth-over-time bench in the suite itself** — `.ai_docs/06-task-graph.md` claims an RSS-growth leak gate exists (R.6) but it is not among any file in `benches/`.
- **No deep-tree (high-depth) cost bench** — every synthetic tree in `nodecost.rs`/`perf.rs` is shallow-and-wide (one container, N flat rows); nothing measures cost as a function of nesting depth, a different cost shape (ancestor-chain hashing, cascading selector resolution) than the flat-row shape every current bench exercises.
- **No text-heavy/CJK/complex-script bench** — all text benches use short ASCII strings; no bench of a text-heavy document or non-Latin shaping cost.
- **No compiled-native competitor benchmark** — despite `plan-node-cost.md` explicitly invoking Makepad's architecture as the design's own motivating comparison, no Makepad, Slint, egui, GTK4, or release-mode Flutter benchmark exists anywhere.

---

## Competitive positioning

**Where Lumen genuinely wins:**
- **Raw throughput against interpreted-language toolkits.** A full 500-row rebuild is 13× faster than GTK3/PyGObject's (776 µs vs 10,190 µs, `docs/comparison-gtk-mintupdate.md:53-54`) — real, but mostly a Rust-vs-Python language-tier effect, not an architecture win, and the project's own doc says so.
- **Startup latency.** 250 ms cold start beats GTK3/PyGObject's 309 ms (which includes real app work Lumen's counter doesn't do) — genuinely favorable, though F23 shows headroom is being left on the table (fully synchronous startup).
- **A single, memory-safe source of truth for four consumers** (render, layout, agent-semantics, snapshot/restore) is a real architectural differentiator no competitor in this space (GTK, Qt, Flutter, egui, Slint) targets — none of them are built agent-observable from the ground up.
- **Identity/addressing scheme.** Typed scope keys are 2.8× faster and allocation-free versus `format!`-based string keys (`docs/plan-hash-identity.md:211-216`) — a genuinely well-executed piece of infrastructure, ahead of what most retained-mode Rust UI frameworks bother to build.

**Where Lumen currently loses, and to whom, specifically:**
- **vs. egui (immediate-mode):** egui doesn't claim O(changed) at all — it accepts full-rebuild cost every frame by design, with extremely cheap per-widget work. As measured (F2), Lumen currently pays *both* the "walk everything" cost of immediate mode *and* the bookkeeping overhead of a retained scene graph, without reliably getting the O(changed) benefit that overhead exists to buy. On many real interaction patterns, egui's honest full-rebuild is very likely cheaper today than Lumen's "incremental" path.
- **vs. Flutter:** Flutter's `setState` scoping is fine-grained at the *widget* level (not app-wide, unlike F3's `allow_copy_forward`), and Skia's rendering does real damage-based repaint end-to-end. Lumen's GPU path currently does not (F1) — this is a structural, not incidental, gap on the single axis ("GPU-composited animation") the project's own comparison doc (§8) names as the reason it isn't competing with GTK on GTK's terms. Flutter is the more credible reference point for that claim today, and Lumen is currently behind it there.
- **vs. Slint / Makepad-style compiled retained-widget frameworks:** these are Lumen's closest architectural peers (compiled, retained, claims of real fine-grained per-property updates) and the *only* ones the project's own design docs cite as the motivating comparison — yet no benchmark against either exists anywhere in the repo. This is the single most conspicuous gap in the competitive story: Lumen is positioned against the toolkit family it's least likely to lose to (interpreted, immediate-invalidation GTK) and never measured against the family it would have to beat to matter.
- **vs. GTK's structural virtualization default:** GTK's `TreeView` cannot accidentally render 500 widgets — the model/view split forces virtualization on the author. Lumen's default (`Scrollable`, F4) can and does. This is the one place the project's own comparison doc identifies as "directly actionable... without giving anything up," and it hasn't been actioned (though the needed widget, `VirtualList`, already exists — this is a packaging/defaults gap, not a missing-engineering gap).

---

## Top 5 highest-leverage optimizations

Ranked by (impact ÷ effort); "effort" accounts for how much of the fix is already designed/scoped in the project's own docs versus net-new engineering.

1. **Wire the already-computed damage into the live GPU present path (F1).** `culled_for_damage`/`damage_between` already exist and are correct — `present_to_surface` just needs to call them and add a scissor rect instead of always doing a full `encode_root`. **Impact:** turns every interactive/animated frame on the path real users actually see from O(scene) to O(damage) — plausibly the single largest win available, since it's currently not a partial win, it's *zero* win on the path that matters. **Effort:** Medium — the diff math exists; the remaining work is making per-layer render targets persistent enough to support `LoadOp::Load` (coupled to F5).

2. **Fix `copy_node`/`copy_span` so a memo hit is cheaper than a full rebuild (F2, F6).** Give `copy_node` a path that reuses the existing taffy `NodeId` via `set_style` (only when the style differs) instead of minting a new tree node and a new taffy node per copied node. **Impact:** self-measured at 1.44× on the framework's own literal acceptance-criterion shape — larger than any other single change the project's own retired N-series plan projected, by the project's own Amdahl analysis (`docs/results-node-cost-n0.md §5`). **Effort:** Medium-High — needs a persistent `LayoutTree` across rebuilds, but the shape of the fix is already scoped in `docs/plan-incremental-path.md` (CP1/CP2).

3. **Decouple `allow_copy_forward` from the app-wide `anims_active()` boolean (F3).** Scope animation-driven memoization suppression to the animating node's ancestor chain/affected spans, using the `span_ctx_hash` infrastructure that already exists per-scope. **Impact:** High for any app using CSS transitions — currently a single hover-fade anywhere disables all memoization everywhere for its duration, which is likely to be *the* common case once transitions are used at all (they're a first-class `.lss` feature). **Effort:** Low-Medium — extends existing per-scope infrastructure rather than building new machinery.

4. **Promote `VirtualList` as the default for long content, and warn on unbounded `Scrollable`/`column` (F4).** The reference implementation (`vlist_1m_scroll`, 1.15 ms/frame) already works. **Impact:** High for the single most common "large content" UI shape — the difference between O(1) and O(N), and the one gap the project's own competitive analysis names as directly closeable without giving anything up. **Effort:** Low — this is a documentation/discoverability/API-promotion task, not new engineering; the hard part (a correct windowing implementation) is done.

5. **Wire CI gating onto `nodecost.rs`/`identity.rs` (F14), not just `perf.rs`'s 5 absolute-latency budgets.** **Impact:** Medium-High indirectly — this is the meta-fix that keeps items 2 and 3 from silently regressing further, or silently "landing" without evidence, and it directly protects the benchmarks that produced this review's most serious findings. Given how much of this review the project already found itself, this is the lowest-risk way to keep that self-correcting capability from degrading. **Effort:** Low — `docs/plan-incremental-path.md` CP0 already scopes ratio-based gates and noise-aware thresholds; this is scripting and threshold design, not new engineering.

*(Runner-up, just outside the top 5 on impact: raise the glyph atlas `max_pages` above 1 and add real eviction (F15) — cheap, but narrower in applicability than the five above, since it only bites text-heavy screens that exceed roughly a thousand unique glyphs.)*
