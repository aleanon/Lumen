# Is there a path to A+ in all five dimensions?

Synthesis of `00`–`04`, graded against `DEFINITIONS.md` (the owner's bars, 2026-08-07).

## The answer

**Yes.** Under the owner's definitions, A+ is reachable in all five dimensions
**without reversing any of Lumen's 21 ADRs and without an architectural rewrite.**
Every gap identified across the five studies is *additive* — work that has not
been done — with exactly one exception, and that exception was removed from the
critical path by the owner's own redefinition.

This is a stronger result than the approved campaign predicted (B/B+ ceiling).
The campaign was not wrong about its own scope; it was scoped to decline exactly
the work that separates B+ from A+.

## Per dimension

| Dimension | A+ bar (owner's) | Reachable? | Binding constraint |
|---|---|---|---|
| **Performance** | Surpass the industry leader on reaction latency, full-view build time, node capacity | **Yes, conditional** | Nothing irreducible. Requires the declined work. ~11-14 person-months — the dominant cost in this plan. **Cannot be graded at all until competitive benchmarks exist.** |
| **Consumer API** | (Not redefined) Agent-writable without docs; failures surface as errors | **Yes** | 26 of 41 unapplied `.lss` properties are mechanical wiring into fields taffy already implements. ~1-2 weeks. |
| **Modularity** | Practically every important part swappable | **Yes** | 2 of 8 axes already done and proven. The remaining 6 need a bundle trait, not 6 more type parameters. |
| **Resource (desktop)** | Configurable span down to constrained targets; beat competitors at default | **Yes** | `<5 MB` is unreachable in *one* configuration (measured 7.46 MB non-font floor). Reachable as a *span* — which is what the owner's definition asks for. |
| **Resource (mobile)** | Same | **Yes for idle/static; unproven for animation** | The 60fps mid-range measurement (CP4) has **never been taken**. Not a known failure — an unknown. |
| **Architecture** | Enables all the above + agent observability + reasonable iteration cycles | **Yes** | Zero ADR reversals. All gaps additive. |

## The spine: modularity is an extension, not a rewrite

The owner's definitions make modularity load-bearing — A+ resource usage is
*defined in terms of* it, and A+ architecture is defined as *enabling* it.

**The pattern already exists and is already proven in production code:**

```rust
// crates/lumen-widgets/src/app.rs:63
pub struct App<R = lumen_render::DefaultRenderer, E = lumen_core::tasks::InlineSpawner>
```

Dual-mode by design: zero-cost generic by default, `Box<dyn Trait>` opt-in. Two
of eight axes (renderer, executor) are done. taffy is already isolated behind
`lumen-layout` with no taffy type escaping the crate — so for layout, the hard
part (isolation) is finished and only the trait is missing.

**The recommended mechanism is not six more type parameters.** `App<R,E,L,T,S,…>`
would wreck build times on a 4,600-line `app.rs` and strain coherence. Instead: a
**Slint-`Platform`-style bundle trait** — one `PlatformConfig` with associated
`Layout` / `Text` / `Style` types — for the pervasively-threaded axes.

**One axis should be deliberately scoped down:** the state store. A full swap
would threaten the exact-attribution property that `ui.getDeps` depends on — i.e.
it would trade the observability pillar for a modularity checkbox. Substitutable
*storage* yes; substitutable *attribution semantics* no.

## Two cross-report disagreements, resolved

### 1. Hot reload: is it a gap of kind?

`00` concluded Tier-2 seamlessness is **architecturally unclosable by any Rust
AOT framework**. `04` found Dioxus `subsecond`'s jump-table mechanism is real and
**already shipping in iced (PR #3000) and Bevy 0.17**, with an integration point
structurally identical to Lumen's `build(cx) -> Element`.

**Both are right about different tiers:**

- **Instance migration** (Flutter/Dart class — preserving live object instances
  across a reload) is a genuine gap of kind for any AOT language: there is no
  precise pointer map. `00` is correct here.
- **Code substitution** (Erlang / Live++ / `subsecond` class) is reachable and
  shipping today. `00` over-generalized by lumping these together.

**And a measurement settles the priority regardless:** Lumen's incremental
rebuild is already **0.4–1.1s** — even with incremental compilation disabled. So
hot-patching would buy *state preservation*, not *speed*. Under the owner's bar
("reasonable iteration cycles"), that loop already qualifies.

**Action: ~1 hour.** Stop `hotpatch.rs`'s hardcoded ABI hash from claiming a
safety property it structurally cannot have. Do not schedule a real Tier-2
integration.

### 2. The retained node graph: cut or keep?

The approved campaign **cut** it. `04` recommends **re-opening** it, and `01`
implies it is required, on a finding that converges across Xilem, Leptos,
Compose and SwiftUI: **O(changed) is a property of a retained target, not of the
reactivity system.** Lumen has the reactive core and lacks the retained tree.

**Resolution: the cut was premature, but the gate was right.** Promote the
campaign's own CP2.3 measurement forward, then let CP5 decide with data. Do not
pre-cut and do not pre-commit — the N-series died of the second, and rev 2 of the
campaign nearly died of the first.

## Corrections that changed the plan

Findings that invalidated prior assumptions, all measured rather than argued:

1. **taffy is not a whole-tree solve.** It has real per-node incremental caching
   (`mark_dirty` + cache; a documented 17s→3ms fix). The decision log killed F2
   because taffy "can't be partially re-solved" — **that premise is false.** Lumen
   discards the cache by minting a fresh `TaffyTree` every rebuild.
2. **"39 silent `.lss` properties" was an artifact** — 89 was a *source line span*,
   not a count. Real: 78 known, 37 applied, **41** unapplied, 26 of them mechanical.
3. **`<5 MB` is unreachable by font policy.** Measured non-font floor **7.46 MB**
   via `size -A` on the stripped binary, cross-validated against the 7.5 MB lean
   figure from an independent angle.
4. **`complex-scripts` isn't the ICU lever** — `icu_segmenter`'s `compiled_data`
   is unconditional in *parley's* manifest.
5. **Per-node memory is a red herring**: a 1,041-node datagrid's Tree+Element
   costs ~1.22 MB against ~270 MB RSS. The GPU-context tax dominates by ~200×.
6. **No production GPU renderer claims cross-vendor bit-exact determinism** —
   Impeller, Skia Gold, WebRender all use tolerance testing. Lumen's ΔE parity
   harness *is* the industry-standard answer, not a self-imposed tax.
7. **`rfd`'s upstream default is now `xdg-portal`**, and the portal path pulls
   `pollster`, not tokio — the 2026-07 reason for choosing GTK is outdated.

## Sequence

Modularity first, because performance and resource grading both depend on it, and
because benchmarking must exist before "surpass the leader" means anything.

**Phase 0 — make grading possible (~1-2 weeks).**
Competitive benchmark, cheap tier: egui, Slint, GTK4-via-gtk4-rs — all runnable
on this box, no new toolchains (~5 person-days). Plus CP0's bench gate and CP4's
never-taken ARM measurement. *Without this, "A" and "A+" are ungradable.*

**Phase 1 — the modularity spine (~4-6 weeks).**
`PlatformConfig` bundle trait; layout and text behind traits; style engine
extension point (`register_property` — the hot-path objection was wrong,
`Style::apply` runs behind a memo cache); shared shell crate (~3-5 days).
This phase *is* the resource-span work — one job, two dimensions.

**Phase 2 — performance to the bar (dominant, ~11-14 person-months).**
Persist the taffy cache (premise-corrected, lower risk than framed); the CP-series
copy-path fixes; damage into the GPU present path; virtualization by default
(which also sidesteps the one irreducible layout constraint, flexbox
sibling-coupling); CP2.3 → CP5 gate on the retained tree.

**Phase 3 — API and observability to A+ (~3-4 weeks).**
The 41 properties (26 mechanical); `SignalKey<T>`; the `ui.explain` primitive —
one principled introspection verb with four resolution kinds, closing all 11
observability blind spots rather than accreting RPC methods.

**Phase 4 — constrained profile and mobile.**
The `no-GPU`/small-memory configuration the modularity spine makes possible;
mobile memory-pressure wiring; first APK/IPA.

## Cost

**Performance dominates: ~11-14 person-months.** Everything else totals roughly
3-4 person-months and overlaps heavily — Phase 1 serves modularity, resource, and
architecture simultaneously.

Do not sum the five reports' estimates; they double-count the shared spine.

## What would make this fail

1. **Competitive benchmarks come back unfavourable in a way optimization can't
   close.** Real risk, unquantifiable until Phase 0 runs. This is the single
   biggest unknown in the plan.
2. **CP5 says stop, and the ratio still disappoints.** Then O(changed) needs the
   retained tree after all, and Phase 2 grows.
3. **Mobile animation performance turns out to be structural.** Currently unknown,
   not unfavourable — CP4 has never been run.
4. **Generic explosion.** If the bundle trait doesn't contain the parameter count,
   build times regress and the AI-iteration bar suffers. Mitigate by measuring
   build time as a gated metric, not an afterthought.

## Bottom line

The framework does not need to be rebuilt to reach A+. It needs the work its own
campaign declined, sequenced behind a modularity spine that is already two-eighths
built, with a competitive benchmark run first so the target is a number rather
than an aspiration.

The one thing that was genuinely unreachable — Flutter-class stateful hot reload —
is the one thing the owner has said does not matter most.
