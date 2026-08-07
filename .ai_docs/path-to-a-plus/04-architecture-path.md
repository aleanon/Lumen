# 04 — Path to A+ Architecture

*Research brief, 2026-08-07/08, revised after the owner's `DEFINITIONS.md`
redefinition of the A+ bars (read first — it supersedes the original brief
where they conflict). Grounds every repo claim in source read directly:
`crates/lumen-cli/src/hotpatch.rs`, `crates/lumen-core/src/identity.rs`,
`crates/lumen-core/src/state.rs`, `crates/lumen-core/src/tasks.rs`,
`crates/lumen-widgets/src/app.rs`, `crates/lumen-widgets/src/a11y.rs`,
`crates/lumen-render/src/lib.rs`, `crates/lumen-agent/src/lib.rs`, the full
ADR table in `.ai_docs/07-decision-log.md`, `docs/results-node-cost-n0.md`,
`docs/plan-incremental-path.md`, `docs/plan-reactive-derive.md`, and the
approved campaign `/home/aleksander/.claude/plans/zippy-dancing-allen.md`
(rev 2) — plus two heavily-sourced background research passes (Rust/native
hot-reload internals across Dioxus subsecond/iced/Bevy/Zed/egui/Erlang/Dart/
Lisp/Smalltalk/Live++/JVM, with measured build-time baselines on this repo;
and Xilem/Leptos/Compose/SwiftUI reactivity internals) and direct fetches of
Slint's `Platform` trait docs. Sibling documents `00-what-is-a-plus.md`,
`02-api-modularity-path.md`, `03-resource-path.md` cover the
competitive-benchmark, consumer-API, and resource-usage dimensions in depth;
this document defers to them on `register_property`, the shared shell crate,
and binary-size mechanics, and focuses on what is specifically architecture's
to answer: the modularity seam, hot reload, observability, reactivity, and
the tree/identity model.*

---

## Verdict

**A+ architecture is reachable, and on the owner's redefined bar — "the
architecture that enables [industry-matching performance, substitutable
internals, and a configurable-to-full-power resource span], while giving
agents proper observability and reasonable iteration cycles" — it is
reachable without reversing a single one of Lumen's 21 existing ADRs.**
Every load-bearing gap found is additive: a new trait, a new ADR recording a
decision the existing ADRs already leave room for, or finishing an
un-scoped-until-now piece of engineering. Nothing requires undoing ADR-007
(fine-grained signals), ADR-008 (tree+SoA, not ECS), ADR-009 (one semantic
tree), ADR-012/013/014 (the three hot-reload tiers and their state
discipline), or any of the others.

That reachability is conditional on four commitments, in order of what the
new bar makes most load-bearing:

1. **Extend the `App<R, E>` generic seam to the six axes that don't have it
   yet**, using a hybrid mechanism (free type parameters for the two axes
   that already have them plus the shell/platform axis; an associated-type
   "backend bundle" for layout+text+style, which are too entangled with
   `app.rs`'s internals to each take their own `App` type parameter without
   wrecking build times; a documented allocator recipe, not new API, for the
   memory-strategy axis). This is the new center of gravity per
   `DEFINITIONS.md` and gets the most space below.
2. **Retain the `Tree`/`LayoutTree` structure across pumps** (the campaign's
   own "CUT: the retained node graph," re-opened) — not because performance
   demands it on its own terms (that question is `DEFINITIONS.md`'s and
   belongs to the competitive-benchmark study), but because it is the same
   piece of engineering the modularity axes (a swappable layout engine, a
   swappable state store) and observability (stable node handles across
   pumps, not per-pump-only) both independently need. Four unrelated
   comparative datapoints (Xilem, Leptos, Compose, SwiftUI) converge on
   "retain a structure, diff/patch cheaply against it" as the only known
   shape that gets O(changed) *and* stays introspectable — this section's
   research is unusually corroborated.
3. **Close the agent-observability blind spots with one queryable causal
   model**, not eleven more RPC verbs bolted on independently — the
   `DEFINITIONS.md` bar names this directly ("proper observability... with
   reasonable iteration cycles") as co-equal with modularity, not
   secondary to it.
4. **Right-size Tier-2 hot reload's honesty, and recognize it is not
   actually where the "iteration cycle" problem lives.** The measured
   evidence (§2) is more decisive than the original brief anticipated: on
   this repo, an incremental `cargo build` after a real code edit is
   **0.4-1.1 seconds**, even with incremental compilation disabled in
   `.cargo/config.toml` and `mold` installed-but-unused. Sub-second Rust
   code reload is achievable via Dioxus `subsecond`'s jump-table mechanism
   — it is shipping in production today in **iced** (not a research
   project) with an integration point structurally identical to Lumen's
   own — but per the owner's redefinition, and per this measurement, it
   would buy Lumen *state preservation across a reload*, not speed Lumen
   doesn't already have. The honest recommendation is to fix the currently
   broken safety claim cheaply and defer the rest.

---

## 1. Modularity — the spine

### 1.1 The existing pattern, precisely

Two axes are already swappable, and they establish a specific, load-bearing
pattern worth naming exactly because the rest of this section is "extend
it," not "invent something new."

```rust
// crates/lumen-widgets/src/app.rs:63
pub struct App<R = lumen_render::DefaultRenderer, E = lumen_core::tasks::InlineSpawner> {
    root: Box<dyn Fn(&mut BuildCx) -> Element>,
    renderer: R,
    // ...
}
```

```rust
// crates/lumen-render/src/lib.rs:69
pub trait Renderer {
    fn render_frame(&mut self, list: &DisplayList, width: u32, height: u32,
                     scale: f64, background: Color) -> RgbaImage;
    fn render_damage(&mut self, list: &DisplayList, width: u32, height: u32,
                      scale: f64, background: Color, dirty: Rect) -> RgbaImage; // default: full render, cropped
}
```

```rust
// crates/lumen-core/src/tasks.rs:158
pub trait Spawner {
    fn spawn(&self, fut: BoxFuture);
    fn spawn_blocking(&self, f: BlockingJob);
}
impl<S: Spawner + ?Sized> Spawner for Box<S> { /* ... */ }  // object-safe by construction
```

The pattern is not merely "a generic parameter." It is **dual-mode by
design**, and the doc comments say so explicitly: *"The runtime is generic
over `R` — zero-cost by default; a consumer who wants dynamic backend
selection uses `R = Box<dyn Renderer>`"* (`app.rs:60-62`). Both traits are
written to stay object-safe (no generic methods, owned return types) so the
`Box<dyn Trait>` escape hatch is always available without a second trait
hierarchy. This gives Lumen something better than either pure static
dispatch (fast, but every backend combination is a distinct monomorphized
type — bad for build times at scale) or pure dynamic dispatch (flexible, but
pays a vtable indirection on every call, unacceptable on `render_frame`'s hot
path): **the default is free, and dynamism is opt-in, per axis, at the
cost of one line at the call site.**

`lumen-layout` already isolates taffy correctly for this pattern to extend
to layout — `rg -rln "taffy::" crates/lumen-widgets/src crates/lumen-core/src`
returns nothing; no taffy type crosses the wrapper crate's boundary. The hard
part of "make layout swappable" (encapsulation) is already done; only the
trait itself is missing.

### 1.2 The mechanism question: more type parameters, or something else?

Naively extending the pattern — `App<R, E, L, T, S, Sty, ...>` — is the wrong
answer, and it's worth being precise about why, because "just add more type
parameters" is the failure mode this section exists to head off.

**Why raw parameter-per-axis breaks the other new bar.** `app.rs` is 4,613
lines and every rebuild pass touches layout, style resolution, and state
addressing in the same function bodies (`build_node`, `rebuild_inner`,
`patch_bg_bindings`). Making each of those a free `App` type parameter means
every one of those internal functions becomes generic over `L: LayoutEngine,
T: TextEngine, S: StateStore, ...` too — Rust monomorphizes each distinct
`(R, E, L, T, S, Sty)` tuple actually instantiated into its own copy of that
4,600-line function graph. With even two implementations per axis that's
2⁶ = 64 possible monomorphizations of the app runtime; in practice only one
or two combinations are ever built per binary, so the *code bloat* risk is
overstated (unused monomorphizations aren't generated), but the **compile-time**
risk is real and directly threatens the "reasonable iteration cycles" bar:
every one of those internal functions gets re-type-checked and re-codegen'd
against six more generic bounds, and rust-analyzer's per-keystroke inference
cost scales with bound count, not just instantiation count. §2.6 below
measures Lumen's current incremental-build baseline at well under a second
per edit — a naive six-parameter `App` is exactly the kind of change that
could erode that baseline, which the new bar makes a real, load-bearing
architecture concern, not a style preference.

**Renderer/Spawner avoid this because they're leaf-called, not
pervasively-threaded.** `render_frame`/`spawn` are called from a small
number of call sites at the edges of the pump loop; the 4,600 lines of
`app.rs` internals never need to *see* `R` or `E` as a bound, only `App<R,E>`
itself does. Layout, text, style, and state are different in kind: taffy
calls happen inside `build_node`, `restyle_subtree`, and `copy_node` — the
same functions the CP-series is already editing — and state addressing
(`Runtime::signal`) is called from essentially every widget's `build()`
closure, i.e. from *user code*, not just from `App`'s internals. Threading a
new generic parameter through user-facing call sites is an API break of a
different magnitude than threading one through `App`'s own struct
definition.

**The resolving mechanism: a bundle trait, à la Slint's `Platform`, not
six more raw parameters.** Slint's own answer to spanning MCU software
rendering through desktop GPU rendering is instructive precisely because it
is *not* "one trait per swappable piece." Slint's `Platform` trait
(`docs.rs/slint/latest/slint/platform/trait.Platform.html`) has exactly
**one required method**, `create_window_adapter()`; renderer backend choice
(femtovg / Skia / software) is a **separate, compile-time Cargo-feature
axis**, not a `Platform` associated type, and the MCU-targeting software
renderer exposes its own narrow trait pair (`LineBufferProvider`,
`TargetPixel`) scoped tightly to "how do I get pixels onto this specific
kind of display," decoupled entirely from `Platform`. Two lessons transfer
directly:

- **Not every axis needs a runtime trait.** Slint's renderer choice is a
  compile-time feature flag with no shared trait at all across femtovg/Skia/
  software — because there is no runtime value in choosing at runtime
  which rasterizer an MCU-vs-desktop binary uses; you know at build time
  which one you're shipping. Lumen's `Renderer` trait is *runtime*-swappable
  because CPU/GPU selection is a real runtime decision for one binary
  (headless CI vs. a live window) — that's a property of Lumen's specific
  use case, not a rule every axis must follow. The allocator axis and (per
  §1.3 below) the layout/text/style axes are closer to Slint's
  compile-time-feature shape than to Lumen's existing runtime-trait shape.
- **Bundle the axes that are pervasively-threaded behind one small trait
  with associated types, not N parameters on `App`.** This is the concrete
  recommendation:

```rust
/// One trait, implemented once per "platform configuration." App<R, E, P>
/// gains exactly one new type parameter, not six.
pub trait PlatformConfig {
    type Layout: LayoutEngine;
    type Text: TextEngine;
    type Style: StyleEngine;
    // The state store is deliberately NOT here — see §1.3, it's the one
    // axis this document recommends NOT making a compile-time choice at all.
}

pub struct App<R = DefaultRenderer, E = InlineSpawner, P = DefaultPlatform> {
    renderer: R,
    spawner: E,
    _platform: PhantomData<P>,
    // internals call P::Layout::compute(...), P::Text::shape(...), etc.
}
```

`app.rs`'s internals become generic over one bound, `P: PlatformConfig`, and
reach the concrete engines through `P::Layout`/`P::Text`/`P::Style` —
monomorphization cost scales with the number of *platform configurations*
actually shipped (in practice: one desktop config, maybe one MCU config),
not with the cross-product of independently-varying axes. This is exactly
the shape wgpu itself uses one level down (`wgpu-hal`'s `hal::Api` trait
bundles `Instance`/`Adapter`/`Device`/`Queue`/`CommandEncoder` as associated
types under one trait per backend, rather than five independent generic
parameters on every wgpu-core type) — reusing a pattern already proven at
scale in a dependency Lumen already ships, not inventing a new one.

**Object-safety note.** `PlatformConfig` itself does not need to be
object-safe (it is used only as a static bound on `App<R,E,P>`, never boxed)
— the individual `LayoutEngine`/`TextEngine`/`StyleEngine` traits *should*
follow `Renderer`/`Spawner`'s existing pattern of staying object-safe
internally, so a consumer who does want runtime backend selection for one of
these axes can still reach for `Box<dyn LayoutEngine>` inside a concrete
`PlatformConfig` impl, without Lumen having to choose statically- vs.
dynamically-dispatched once and for all.

### 1.3 Per-axis assessment

| Axis | Today | Mechanism | Cost | Blocker class |
|---|---|---|---|---|
| **Renderer** | Done — `trait Renderer`, `App<R,...>`, two backends | — | — | done |
| **Executor/Spawner** | Done — `trait Spawner`, `App<...,E>`, object-safe | — | — | done |
| **Layout engine** | taffy concrete, but already isolated behind `lumen-layout` (no taffy type escapes) | `trait LayoutEngine` in the `PlatformConfig` bundle (§1.2); taffy becomes the default impl | **Small-medium.** The encapsulation work (the hard part) is done. Extracting a trait from taffy's existing call surface in `lumen-layout/src/tree.rs` is mechanical — ~3-5 days, similar shape to `register_property`'s costing in `02-api-modularity-path.md` §5 | **unfinished** |
| **Text/shaping engine** | parley/swash concrete, no wrapper crate at all (unlike layout) | First isolate behind a `lumen-text` boundary (parley types must stop leaking — check needed, likely leaks today since there's no wrapper discipline enforced), then a `trait TextEngine` in the bundle | **Medium-large.** Two steps, not one: encapsulation *then* abstraction, and text shaping's cache (glyph atlas, shaped-run cache) is more deeply wired into the GPU path than layout's output is — swapping shaping engines while keeping the atlas cache coherent needs care | **unfinished, but larger than layout** |
| **Style engine** | `Style::apply` a closed `match`; `register_property` already designed in `02-api-modularity-path.md` §5 (registry + `Style` side-table, ships without touching the closed `Value` enum) | Not a new trait — `register_property`'s registry *is* the extensibility mechanism for this axis; `PlatformConfig::Style` would be the wrong tool (style extensibility is per-property, not per-engine) | ~3-5 days, already fully costed in the sibling document | **unfinished, already scoped elsewhere** |
| **State store** | `Runtime` concrete; `ReadCx`/`WriteCx` traits exist but the store itself does not | **Recommend: do not make this a compile-time-swappable axis for 1.0.** See below | High if attempted broadly; near-zero if scoped correctly | **irreducible in the short term** (see below) |
| **Allocator/memory strategy** | No lever at all today | Not an `App` type parameter — Rust's `#[global_allocator]` is already the correct, orthogonal mechanism. Lumen's job is a *documented recipe* (which allocator to plug for an MCU/no_std floor — e.g. `embedded-alloc`/`talc`), not new trait surface | **Low** — this is a resource-usage/docs deliverable, not an architecture change | **unfinished, but cheap** |
| **Shell/platform** | Four separate crates, no shared trait (F7 in the 2026-08 review); `02-api-modularity-path.md` §5 already designs and costs a `lumen-shell-core` (~3-5 days) covering the iOS/web-shared 23-line `render_into` duplication | The shared-shell-crate design already exists; extend it with the *same* `PlatformConfig`-shaped bundle idea only if a fourth platform's needs diverge enough to justify a trait rather than a shared helper crate — not needed yet | ~3-5 days, already scoped | **unfinished, already scoped elsewhere** |

**Why the state store is different in kind, and the recommended scope.**
Every other axis is substitutable because the *rest* of the system treats it
as an opaque capability (paint a display list; compute a layout; shape some
text). The state store is not opaque to the rest of the system — `IdHash`,
`Runtime::signal_at`, snapshot serialization (ADR-011), agent dependency
introspection (`ui.getDeps`/`ui.whatDependsOn`, ADR-009), and the entire
hot-reload `Checkpoint` protocol (ADR-013/014) are all written against the
concrete shape of `Runtime`'s slot table and `IdHash`'s 128-bit folding
scheme. Making the store itself a type parameter would mean every one of
those subsystems becomes generic over `S: StateStore`, and — unlike
layout/text, which only need to answer "what are the final bounds" or "what
glyphs to draw" — the agent-observability contract (ADR-009, the project's
actual differentiator) depends on the store's *internal* shape (fold-based
identity, versioned slots, read-set collection during build) being known,
not just its external behavior. A `StateStore` trait abstract enough to
admit a genuinely different implementation (say, a positional/slot-table
scheme like Compose's) would either (a) leak that implementation's specific
shape into the trait, defeating the point, or (b) be so abstract it can't
support `ui.getDeps`'s exact-attribution guarantee, which is exactly the
axis `DEFINITIONS.md` says the architecture must keep. **Recommendation:**
scope "state store modularity" down to what the resource-usage bar actually
needs — a **constrained-memory implementation of the same `Runtime` shape**
(e.g., a slot table backed by a fixed-capacity arena instead of a growable
`HashMap`, for the no_std/MCU floor), reachable via a feature flag or a
smaller internal trait (`SlotStorage`) that `Runtime` is generic over
*internally*, not by making `Runtime` itself swappable for a structurally
different reactivity model. This satisfies "configurable floor... reachable
by type parameters and features, not by forking" (`DEFINITIONS.md`) without
touching the part of the store that observability depends on.

### 1.4 Resource usage as a consequence of modularity

`DEFINITIONS.md`'s resource-usage bar requires two things simultaneously: a
**configurable floor** reachable without forking, and a **competitive
default**. The modularity design above is what makes the floor reachable —
concretely, a "constrained" `PlatformConfig` would pick: `Renderer =
TinySkia` (already the CPU default), `Spawner = InlineSpawner` (already the
default), a fixed-capacity `Runtime` (§1.3's `SlotStorage` scope), and (once
built) a smaller `LayoutEngine`/`TextEngine` pairing, plus a documented
`#[global_allocator]` swap. None of that requires a fork — every piece is
already either a default, a feature flag, or (post-§1.2/§1.3) a type
parameter. The *default* configuration stays exactly what it is today
(`DefaultRenderer` = wgpu-capable, growable `Runtime`, full taffy/parley),
so "defaulting to a full-power configuration" is satisfied by construction,
not by a separate code path to maintain. Slint's own `Platform`/renderer
split is the existence proof that one codebase can genuinely span both ends
without the low end being a fork — the mechanism above is Lumen's version of
the same idea, adapted to a runtime-generic rather than compile-time-feature
default for the two axes (renderer, executor) where Lumen already made that
call for good reasons (headless CI needs runtime CPU/GPU selection; Slint's
MCU target does not need runtime renderer selection at all).

### 1.5 Cost summary

| Item | Effort | Depends on |
|---|---|---|
| `PlatformConfig` bundle trait + `App<R,E,P>` | ~1 week (design + threading `app.rs`'s internals through one new bound) | none |
| `trait LayoutEngine`, taffy as default impl | ~3-5 days | `PlatformConfig` |
| `lumen-text` encapsulation (stop parley/swash leaking) + `trait TextEngine` | ~1.5-2 weeks (encapsulation first, abstraction second) | `PlatformConfig` |
| `register_property` (style axis) | ~3-5 days — already costed in `02-api-modularity-path.md` §5 | none, parallel |
| `SlotStorage` internal trait for `Runtime` (constrained-memory floor only) | ~1 week | none, parallel |
| Allocator recipe (docs + one example, `#[global_allocator]`) | ~2-3 days | none, parallel |
| `lumen-shell-core` (shell axis) | ~3-5 days — already costed in `02-api-modularity-path.md` §5 | none, parallel |
| **Total, one engineer, sequential** | **~6-7 weeks** | |
| **Total, 2-3 engineers on independent tracks** | **~3-4 weeks** | |

No ADR reversal anywhere in this list. ADR-004 ("Layout: Taffy behind
`lumen-layout` wrapper... extensions implemented in the wrapper") already
anticipates exactly this; a `LayoutEngine` trait is the wrapper crate's
natural extension, not a departure from it.

---

## 2. Hot reload — right-sized, not the critical path, and more decisively answered than expected

*Per `DEFINITIONS.md`: demoted — "hot-reload speed is a plus, but not the
most important thing." Reported in full because the research is unusually
conclusive: it settles the sibling study's "one gap of kind" framing with a
sharper boundary, and it produces a measurement that changes the actual
recommendation. Read as background, not as proposing new critical-path
work.*

### 2.1 The three-capability decomposition, and where the wall actually is

Background research (primary sources: Dioxus `subsecond`'s own docs and
source, fetched directly; the Dart SDK's `runtime/docs/hot-reload.md` and
`isolate_reload.cc`; Erlang/OTP's `code_ix.h`/`export.h`; CLHS §4.3.6;
Pharo's `ProtoObject`/`ShiftClassInstaller` source; Live++'s documentation;
JVMTI's spec) decomposes "hot reload" into three separable capabilities
usually discussed as one:

| capability | what it means | who has it |
|---|---|---|
| ① code substitution | swap a function's implementation for new code | Dart VM, Erlang/BEAM (incl. its native JIT, BeamAsm), JVM HotSwap/DCEVM, Live++, **Dioxus subsecond**, `hot-lib-reloader` |
| ② state retention | old data survives the swap unchanged | all of the above, to varying degrees |
| ③ shape migration + identity | *every live instance* of a changed type is found on the heap and its fields remapped by name, preserving `eq`/pointer identity | Dart VM (`InstanceMorpher` + `become`), Smalltalk (`become:`), Common Lisp (CLOS `update-instance-for-redefined-class`) — **and no one else surveyed** |

Capability ③ is what Flutter's stateful hot reload actually rests on, and it
is not "hard for Rust" — it is **undefined** for any AOT, non-managed
runtime. It requires a precise pointer map (every live reference to an
object, findable by a heap walk) — the same primitive a precise tracing
garbage collector needs, and the same reason Rust doesn't have one of those
either. The Rust Reference's own type-layout page states that under the
default representation, type layout "can be changed with each compilation,"
guaranteeing only alignment and non-overlap — field order, padding, and
offsets are explicitly unspecified (`doc.rust-lang.org/reference/type-layout.html`).
**Two compilations of unchanged source can disagree on layout**, which is
the specific fact that condemns `hotpatch.rs`'s `HOST_ABI_HASH: u64 =
0x1111_2222_3333_4444` design at the category level, not just the
implementation level: even a *correctly computed* struct-layout hash would
be comparing two numbers the language does not promise will agree, so no
hash-based gate can ever be sound for this purpose.

**This confirms, and sharpens, the sibling study's "one gap of kind"
finding.** Capability ③ is unreachable by Rust or any AOT language without a
fundamentally different runtime (a tracing GC, or Smalltalk/Lisp-style
image-based execution) — a gap of kind, full stop, with no counter-example
found anywhere in the survey.

### 2.2 What subsecond actually does — and the load-bearing discovery: it's already shipping in a peer Rust GUI framework

Fetched directly from `docs.rs/subsecond` and its source:

- **Mechanism: a global jump table + function-pointer indirection, not
  binary patching.** Verbatim: *"Subsecond works by detouring function
  calls through a jump table... Subsecond does not modify your process
  memory."* A patch is a **new shared library loaded alongside the running
  process**, ASLR-rebased against the running binary's own `main` symbol,
  and published atomically to a global `AtomicPtr<JumpTable>`.
- **No ABI fingerprint exists, by design — and this is the more important
  finding than "Lumen's hash is a placeholder."** Subsecond doesn't check
  layout compatibility at all; it makes the question moot by re-driving the
  *same* rustc against the *same* running binary's object files, diffing
  compiled output and invalidating only functions that actually changed.
  That is **structurally stronger than any hash could be**, not merely a
  different implementation of the same idea — it eliminates the failure
  mode a hash exists to catch, at the cost of requiring the patch to come
  from the same build session, not an independently-fingerprinted rebuild.
  For **data**, it gives no guarantee and says so explicitly: *"frameworks
  that implement subsecond patching properly will throw out the old
  state."* This directly reframes the correct fix for `hotpatch.rs`: the
  category error is comparing hashes at all, not the specific hash chosen.
- **Explicit, hard limits, verbatim from source:** cannot hot-reload struct
  layouts ("the generated code assumes a particular layout and alignment");
  generic-forwarding functions are unsupported ("changes to functions that
  forward generics can cause a cascade of codegen changes"); a change
  *above* a wrapped call site triggers unwinding to the nearest outer
  `subsecond::call()` boundary rather than patching a live frame; only the
  binary crate ("tip crate") was reliably patchable until a 2026-02
  workspace-support merge that itself still leaves **indirect** workspace
  dependencies unwatched (`dioxus#5314`) — directly relevant to Lumen's own
  16-crate, ~51-example workspace shape.
- **Zed evaluated it and declined, for exactly the reason that matters
  most for Lumen.** Zed's own PR (`zed#41508`) wrapped `render`/`paint`/
  `prepaint`/`request_layout`, then was closed by a maintainer: *"we're not
  going to move forward with it until subsecond / rustc are able to support
  workspaces... Only direct workspaces work, not indirect ones."* This is
  the single most relevant negative data point in the whole survey — a
  large, multi-crate Rust GUI codebase hit the same wall Lumen's own
  16-crate structure would hit.
- **iced already shipped it, in production, with an integration point
  structurally identical to Lumen's own.** `iced#3000` ("Hot Reloading"),
  merged 2025-06-24, closing a six-year-old open issue. iced's `view()` —
  functionally the same role as Lumen's `build(cx) -> Element` — is wrapped
  in a ~60-line `debug::hot()` helper (feature-gated, a no-op when the
  `hot` feature is off) built on subsecond via a community-maintained
  standalone build server (`cargo-hot`, since ThinLink, subsecond's actual
  patch-compiler, is embedded in the Dioxus CLI and not published as a
  reusable library). iced's own release notes are appropriately blunt:
  *"Very experimental! May crash your OS. Only changes to the root crate
  will trigger a reload. Changes to your application `State` or `Message`
  types will need a cold restart (for now!)."*
- **Bevy 0.17 shipped the same mechanism for ECS systems**, behind a
  `hotpatching` cargo feature, with its own design doc explicitly scoping
  out struct/enum field migration, global statics, and system
  reordering — the same boundary subsecond itself draws.
- **egui has no integration at all** — an open feature request
  (`egui#5561`) with zero maintainer commitment and zero code hits for
  `subsecond` in the egui repository; only third-party example projects
  demonstrate wiring it in externally.

### 2.3 The measured baseline that changes the recommendation

The most consequential finding of this section is not about subsecond at
all — it's a direct measurement of Lumen's own current incremental-build
latency, run on this repo, this machine, warm caches:

| Scenario | Time |
|---|---|
| True no-op `cargo build -p accordion` | 0.09–0.12 s |
| Edit a 128-line example crate → rebuild | 0.43–0.47 s |
| Edit the same → rebuild a **windowed** example (winit + wgpu linked) | 0.67–0.73 s |
| Edit `lumen-widgets` (17k LOC) → rebuild a dependent example | 0.64–0.70 s |
| Edit `lumen-core` → rebuild 6 framework crates + an example | 1.00–1.06 s |
| Headless app process start + full build/layout/render/PNG | 0.06–0.08 s |

**And this is before any standard acceleration**: `.cargo/config.toml:14-15`
sets `CARGO_INCREMENTAL = "0"` (a deliberate choice, per prior disk
incidents recorded in project memory), `/usr/bin/mold` is installed but not
configured as the linker, and Cranelift's codegen backend isn't in use —
each a well-documented, low-effort lever for further reducing this baseline
further if it were ever needed.

**This directly answers `DEFINITIONS.md`'s "reasonable iteration cycles"
bar, and the answer is: it is already met, by the ordinary edit-rebuild-
restart loop, without hot reload of any tier.** Sub-second round trips on a
16-crate, ~68,000-line workspace is not a gap needing subsecond-class
machinery to close — it is the baseline `subsecond` would be layered *on
top of*, and its actual marginal value in that context is narrow and
specific: **preserving in-memory application state (scroll position,
navigation depth, form contents) across a reload**, not reducing the
reload's latency, because the latency is already small. Tier 3's
`Checkpoint` protocol (serialize → restart → rehydrate,
`crates/lumen-cli/src/dev.rs:130-146`) already serves exactly that need,
at a cost (~2-5s per the architecture doc's own tier table) that is real
but bounded, and — critically — is *sound*, unlike Tier 2's current gate.

### 2.4 Why Lumen's own architecture is unusually well-suited to this, if it's ever revisited

Three of Lumen's own existing invariants are precisely the preconditions
subsecond's documented failure modes require, independently confirmed by
this pass:

1. **ADR-013's "no closures in stored state" is exactly what subsecond
   needs.** Subsecond's own documented hazard is that it does not version
   function pointers by content — a stale `Rc<dyn Fn>` held in durable
   state would silently keep running old code after a patch. Lumen
   structurally cannot have this problem: handlers live on the ephemeral
   element/node graph, rebuilt from the store, never in `Runtime`'s
   serializable slots. ADR-013's own rationale text already names this as
   "the hard precondition for tiers 2–3... and ADR-014."
2. **The Tier-3 Checkpoint protocol is the exact escape hatch subsecond's
   own documentation says a consuming framework must supply and subsecond
   itself will not** ("frameworks... will throw out the old state"). Lumen
   already has it, built and tested, for a different reason (ABI-crossing
   changes) — it would not be new work to reuse it as subsecond's
   fallback for struct-shape changes.
3. **`App::force_full_repaint()` already exists and is already wired into
   the Tier-2 path in `dev.rs`** — the "patch applied → rebuild the tree"
   leg of any future real integration is already built for the fixture
   demo; extending it to flush the F1/F3 memo caches (which hold
   `Rc<dyn Fn>` handlers across pumps and would need explicit invalidation
   on a real patch, unlike the current label-swap fixture) is additive
   work, not a redesign.

### 2.5 Concrete recommendation, at the now-correct priority

1. **Stop the hardcoded ABI hash from claiming a safety property it
   cannot have — do this regardless of hot reload's priority, because it
   is currently a silent-UB trap, not merely an unfinished feature.**
   Given §2.2's finding that even a well-computed hash is unsound for this
   purpose (Rust doesn't guarantee cross-compilation layout stability),
   the right fix is not "compute a better hash" but **remove the pretense
   and always downgrade to Tier 3** until/unless a real integration (§2.4)
   is undertaken. Effort: ~1 hour (delete the comparison, always return
   `Swap::NeedsTier3`, update the fixture tests and docs accordingly).
2. **Do not invest in a real Tier-2 integration now.** §2.3's measurement
   removes the speed argument; §2.2's Zed precedent flags workspace-scale
   risk matching Lumen's own shape; and per `DEFINITIONS.md` this is
   explicitly not the priority. If ever revisited, the shape is already
   proven cheap by iced's shipped example: wrap the one call site at
   `build(cx) -> Element`'s invocation (a single call site in `app.rs`, by
   the same "one closure, called from one place" property that makes
   Lumen's authoring API clean) in a `subsecond::call`-style helper,
   feature-gated exactly like iced's `hot` feature, plus explicit F1/F3
   memo-cache invalidation on patch. Realistic scope by direct analogy to
   iced's shipped diff: **~60-100 lines**, not a subsystem — but building
   the patch-server side (`cargo-hot`'s own README calls itself "Very
   experimental... Will eat your laundry") or depending on the Dioxus CLI
   are both real, non-trivial dependencies to accept for a capability
   `DEFINITIONS.md` has already deprioritized.
3. **"Reasonable iteration cycles" is answered, not by Tier 2, but by
   what's already measured plus Tier 1.** Tier 1 (`.lss`/asset reload) is
   real, cheap, and already measured per-reload
   (`ReloadResult.duration_ms`, `crates/lumen-cli/src/dev.rs:33-40`); §2.3
   establishes that the code-edit path is *already* sub-second without any
   hot-reload machinery. The one remaining open question this document
   flags rather than answers: whether `lumen-test`'s full headless
   verification loop (not just the build) is similarly fast end-to-end on
   a realistic app — worth a follow-up measurement, but not a
   architecture-level gap on the evidence gathered here.

No ADR reversal here. ADR-012 ("abi_hash mismatch auto-downgrades tier 2→3")
is *already* the correct policy — item 1 above makes `hotpatch.rs` actually
implement what ADR-012 already says, rather than changing the ADR.

---

## 3. Agent observability — the complete gap list and a principled design

*Elevated by `DEFINITIONS.md`, which names it directly as co-equal with
modularity in the architecture bar.*

### 3.1 The complete list

Grounded in the 2026-08 review's own 11-item enumeration
(`.ai_docs/review-2026-08/05-architecture.md`, "Agent-observability blind
spots"), independently re-verified against `crates/lumen-agent/src/lib.rs`'s
RPC dispatch table (30+ verbs: `ui.getTree`, `ui.getStyles`, `ui.getDeps`,
`ui.whatDependsOn`, `ui.lastChange`, `ui.getLayout`, `ui.screenshot`,
`ui.lint`, `ui.probe(Region)`, `input.*`, `app.*`, `clipboard.*`), and
`crates/lumen-core/src/tree.rs`'s `hit_test` internals (confirmed: the
candidate walk that would explain a miss exists internally and is discarded,
returning only the winning `NodeIndex` or `None`):

1. **Why a style rule lost the cascade.** `ui.getStyles` returns the winning
   value + origin/span; not the rejected candidates or *why* they lost
   (selector didn't match / matched but lower specificity / property parses
   but the runtime never applies it — a real, distinct category per the
   `styling-lss` skill).
2. **Why a click did nothing, at the hit-test level.** `Tree::hit_test`
   (`crates/lumen-core/src/tree.rs:253`) walks all candidates internally and
   returns only the winner or `None`; no RPC path surfaces "there was a node
   there but it was clipped / not `HIT_TESTABLE` / occluded by a higher-z
   sibling."
3. **Event routing/bubbling trace.** No verb reports dispatch path, whether
   propagation stopped, or which handler stopped it — only before/after tree
   diffs are inferable.
4. **Layout reasoning vs. layout results.** `ui.getLayout` returns final
   bounds/ink-bounds/clip/text-metrics but not *why* — which constraint
   bound the width, whether a child was compressed below intrinsic size.
   Taffy has this internally; none of it crosses the RPC boundary.
5. **Animation/transition state mid-flight.** No verb for "is an animation
   running on node X, what's its progress, when does it settle" — only
   `ui.waitSettled`'s pass/fail.
6. **IME composition state.** Real IME wiring exists in the shell but no
   RPC verb surfaces mid-composition candidate text/range.
7. **Focus/hover/drag as a named, O(1) query.** `NodeFlags::FOCUSED/HOVERED/
   PRESSED` exist; no "what has focus right now" verb — an agent walks the
   whole tree and filters.
8. **Panics inside a contained `error_boundary` are invisible to RPC.**
   `app.logs`/`app.diagnostics` surface top-level build panics (`E0701`);
   a *contained* panic (the intended failure mode) renders a fallback
   silently, with no diagnostic — an agent has to grep rendered text for a
   "⚠" marker.
9. **The live AccessKit tree is unverifiable from the agent surface.** No
   verb fetches the actual built `TreeUpdate` for diffing against
   `semantics_json` programmatically.
10. **Reactive-dependency verbs are `snapshot`-feature-gated, all-or-nothing.**
    `ui.getDeps`/`ui.whatDependsOn` are `#[cfg(feature = "snapshot")]`-only
    (`app.rs:2977,3006`) — the lean profile the modularity/resource work
    above pushes as the configurable-floor default has **zero** agent
    introspection by construction. Also: root-level reads not inside a
    scope/binding aren't attributed to any node, so global state changes
    are invisible to `whatDependsOn`.
11. **No structured tree-diff verb.** A structural rebuild reports only
    `rebuild` (kind, not per-node diff) via `ui.lastChange`; an agent must
    diff two full `ui.getTree` snapshots itself for the common case.

### 3.2 Why "eleven more RPC verbs" is the wrong shape of fix

Every item above is individually cheap (the review's own assessment, and
independently confirmed here — each extends a data structure that already
exists: `computed_json_spanned`'s origin/span for #1, `hit_test`'s internal
candidate walk for #2, taffy's internal constraint trace for #4). But
patching them one at a time as independent RPC verbs is exactly the
"accreting RPC verbs" anti-pattern the brief warned against — eleven
special-purpose queries is not an observability *architecture*, it's eleven
observability *features*, and the twelfth blind spot found next quarter gets
verb #31.

### 3.3 A principled design: one causal query, four evidence kinds

The common shape underneath all eleven items is the same question asked
about different subsystems: **"what candidates existed, which one won, and
why did the others lose?"** Style cascade (#1), hit-testing (#2), event
routing (#3), and layout constraint resolution (#4) are all, structurally,
*a resolution process over competing candidates that already runs and
already discards its losing candidates* — the fix in every case is "stop
discarding," not "build a new subsystem." This suggests one new primitive
rather than four:

```rust
/// A generic "why" record: what was considered, what won, and the reason
/// each loser lost. Every resolution process below already computes b) and
/// c) internally and already throws them away; this makes that discarding
/// opt-in (only when explain=true; §3.4 addresses the cost) rather than
/// unconditional.
pub struct Resolution<Candidate, Reason> {
    pub winner: Option<Candidate>,
    pub rejected: Vec<(Candidate, Reason)>,
}
```

One new RPC verb, parameterized by *what kind of resolution* to explain,
rather than one verb per subsystem:

```
ui.explain { node?: Selector, point?: {x,y}, kind: "style" | "hitTest" | "layout" | "event" }
  -> Resolution<...>
```

- `kind: "style"` on a node → the existing `computed_json_spanned` winner,
  plus every candidate rule that matched the selector at all, each tagged
  with why it lost: `Overridden{by}` / `LowerSpecificity` / `NotApplied`
  (closes #1, and specifically the *"the parser accepts it, the runtime
  doesn't apply it"* category the `styling-lss` skill already documents as
  real but currently undetectable at the RPC layer).
- `kind: "hitTest"` at a point → `Tree::hit_test`'s already-computed
  candidate list, each tagged `Clipped` / `NotHitTestable` / `Occluded{by}`
  / `Won` (closes #2).
- `kind: "layout"` on a node → taffy's per-node constraint trace (which
  constraint bound each axis, whether the node was compressed below
  intrinsic size) (closes #4).
- `kind: "event"` on a node → the dispatch path for the last event routed
  through that node, with `stoppedAt`/`handledBy` (closes #3).

This is additive to `03-spec-semantics-agent.md`'s existing schema (a new
verb, no change to `ui.getTree`/`ui.getStyles`'s existing shape) and reuses
data every one of the four subsystems already computes and currently frees —
this framing directly matches the review's own corrective note for F6
("extending the cascade evaluator to also record rejected candidates is
additive to that existing data structure, not a new subsystem"), generalized
across all four resolution processes instead of stopping at style.

**The remaining seven items don't fit the "resolution" shape and need their
own additions, but each is still additive, not architectural:**

- #5 (animation progress), #6 (IME state), #7 (focus/hover/drag as O(1)):
  these are **state snapshots**, not resolutions — a single
  `ui.getInteractionState()` verb returning `{focused, hovered, pressed,
  dragging, animations: [{node, progress, eta}], ime: {composing,
  candidateText, range}}` in one O(1) call covers all three, because all
  three already live as fields on retained structures (`NodeFlags`, the
  animation engine's `AnimVal` table, the IME shell state) — the fix is
  exposing one struct, not three verbs.
- #8 (contained-panic visibility): `error_boundary` should push a
  structured diagnostic (a new `W`-code, "subtree degraded, panic
  contained") into the same `app.diagnostics()` stream top-level panics
  already use, rather than silently rendering a fallback. Cheap, additive,
  no new verb needed.
- #9 (live AccessKit tree): a `ui.getAccessKitTree()` verb that serializes
  the actual built `TreeUpdate` for diffing against `semantics_json` — this
  one genuinely is a new, narrow verb, because AccessKit's `TreeUpdate` is a
  different schema from Lumen's own semantics JSON and there's no existing
  data structure to reuse.
- #10 (snapshot-gated introspection in the lean profile): resolved by
  §1.4's modularity work, not by observability work — a distinct "agent"
  point on the feature matrix (small binary, `snapshot` on, everything else
  off) rather than collapsing lean-vs-full into two points, per
  `02-api-modularity-path.md`'s own tension #1 and mitigation.
- #11 (structured tree-diff): the biggest genuinely-new piece. A `patch`
  report already carries exact node ids for the surgical binding-only path;
  extending that same `ChangeReport` shape to a full structural rebuild
  (walk old-tree-vs-new-tree once, at rebuild time, since both trees briefly
  coexist as `prev_tree`/`tree` — `app.rs`'s own diff, not a re-derivation)
  closes this without a new subsystem, but is real work — this is also
  where the retained-tree work in §4 pays a second, unrelated dividend: a
  tree that survives across pumps makes "what changed since last pump" a
  question the retention bookkeeping is already answering for the copy-node
  path, rather than a separate diff pass.

### 3.4 Cost, and the performance objection addressed up front

Every `explain`-kind resolution is opt-in per call (`ui.explain` is not
called on the hot path — it's an agent-initiated debugging query), so
"stop discarding the losing candidates" only needs to happen inside the
already-slow-path code (parse-time cascade evaluation, the hit-test's own
existing candidate walk) gated behind a flag the RPC dispatcher sets before
re-running that specific resolution on demand, not unconditionally on every
frame. This mirrors `02-api-modularity-path.md`'s own finding for
`register_property` (the relevant call sites are memo-gated, not per-frame)
— the same "pay for what you use" shape applies here by construction, since
`ui.explain` literally re-invokes the resolution function with a flag
flipped, rather than the render loop paying to always collect rejected
candidates.

**Effort:** `ui.explain` (four kinds, reusing existing internal data) ~1-1.5
weeks; `ui.getInteractionState` ~2-3 days; error-boundary diagnostic wiring
~1 day; `ui.getAccessKitTree` ~2-3 days; the agent-profile feature-matrix
point (§1.4/§3.3 item #10) ~1-2 days once the `PlatformConfig` work lands;
structured tree-diff for the rebuild path ~1 week standalone, cheaper if
sequenced after §4's retention work. **Total: ~4-5 weeks**, most of it
parallelizable with the modularity work in §1 since it touches different
files (`lumen-agent`, the cascade evaluator, `hit_test`) rather than
`app.rs`'s rebuild core.

---

## 4. Reactivity — keep the model, fix the target, in that order

### 4.1 What the comparative research settled

The background research (primary sources: Xilem's own `ARCHITECTURE.md` and
`xilem_core` source; Leptos's `tachys` renderer source; AOSP's
`SlotTable.kt`/`Composer.kt`; Apple's WWDC21/WWDC23 transcripts; Raph
Levien's own design posts) converges, independently, on one architectural
fact:

**O(changed) is a property of a retained target that survives the update,
not of the reactivity system that decides what changed.** Every framework
surveyed that achieves it — Xilem (`Masonry` widget tree, retained across
cycles), Leptos (the actual DOM, owned by the browser, never destroyed —
`tachys::Render::State` holds a live node handle and `rebuild` mutates it in
place), Compose (the slot table, a gap-buffered array surviving across
recompositions — a skip is one integer add, `currentGroup += groupSize`),
SwiftUI (the attribute graph + `@State` storage, permanently attached to a
stable identity) — pays its per-node structural cost **once, when the
structure actually changes**, against something that is still there next
frame. Lumen's `Tree`/`LayoutTree` are discarded and rebuilt from scratch
every pump (`app.rs:2494`, `self.prev_tree = std::mem::replace(&mut
self.tree, Tree::new())`), so a "memo hit" in `copy_node` cannot be a cheap
mutation — it can only be a *copy of saved parts into a freshly-minted
destination*, and copying has an unavoidable floor (structural insert + a
fresh taffy node + four side-table re-keys) no amount of bookkeeping
optimization can go beneath. That floor, not a bug in `copy_node`'s
specific hashmap choreography, is what `docs/results-node-cost-n0.md`
measured as a 1.44× pessimization.

**This is not a case for abandoning ADR-007.** Leptos is the load-bearing
counter-example to "retention means a VDOM": its `RenderEffect` holds
retained per-node state and a signal write patches it directly, with zero
diffing anywhere in that path — proving retention and diffing are
orthogonal. Lumen can retain the `Tree`/`LayoutTree` across pumps and keep
*zero* diff step: a signal write already knows, via the existing read-set
attribution, exactly which retained node(s) to patch. ADR-007's actual
constraint ("no VDOM/diffing") is preserved; what changes is only whether
the *target* of a patch survives to be patched, versus having to be
reconstructed as a prerequisite to patching it.

**This is also not a case for the retired `#[derive(Reactive)]` (RD-series)
authoring model.** That plan's three failure reasons (root-only derive
identity; widget-owned string-keyed state doesn't disappear; `evict_scope`
leaks derive-owned list rows) are about the *authoring* API — whether state
is addressed via `cx.signal(key, ...)` or via `&mut` field accessors on one
owned struct — and are completely orthogonal to whether the `Tree` a
rebuild writes into is retained or discarded. Nothing in this section
revisits that retirement.

### 4.2 The concrete recommendation, sequenced against the campaign's own gate

The campaign (`zippy-dancing-allen.md`, M-C) already has the right
diagnostic instinct — CP2.3 explicitly proposes measuring "the taffy-node
mint cost in isolation... if it is <5% it is not worth the retention
machinery" before committing to anything — but sequences it *last*, after
CP1/CP2's bookkeeping fixes, and the CUT decision text for the retained node
graph cites a number ("~1.6pp of a 60Hz frame at 500 nodes") the plan
document itself flags as having "no derivation anywhere in the docs." Given
that the cross-framework evidence above makes the direction of CP2.3's
answer predictable (every comparator retains exactly the structure Lumen's
`Tree`/`LayoutTree` currently discard, specifically to avoid this cost), the
recommendation is:

1. **Run CP0/CP1 first, unchanged from the campaign** — the O(scopes²)
   `prev_spans` scan fix and the ratio-gate infrastructure are correct and
   architecture-neutral regardless of what CP5 eventually decides.
2. **Promote CP2.3 (the isolated taffy-node-mint measurement) to run
   immediately after CP1, not after CP2's bookkeeping changes.** It is the
   one number that actually decides the architecture question, and running
   it early de-risks committing engineering time to CP2's bookkeeping
   changes if the answer turns out to make them moot.
3. **Re-open the CUT decision as a scoped increment, not the XL "retained
   node graph."** The right-sized version already has a name in the
   project's own docs: `plan-retained-pipeline.md`'s **A.3.3 splice-in-place**,
   deferred at the time on the reasoning that it would "buy... skipping the
   O(tree) shallow walk — a perf refinement, not a capability." The N0 data
   overturns that specific judgment (the shallow walk *is* the dominant
   cost on a memo hit) without requiring anything as large as building a
   new retained-node-graph subsystem from scratch: retain the existing
   `Tree` arena and taffy `LayoutNode` identities across pumps so a
   memo-hit span becomes a **no-op** (zero inserts, zero taffy mints, zero
   side-table re-keys), gated by the F0 coherence oracle that already
   exists (`assert_view_coherent`).
4. **This is also the prerequisite the modularity (§1) and observability
   (§3.3, item #11) sections independently need**, which is the strongest
   argument for doing it now rather than deferring further: a `LayoutEngine`
   trait is far more natural to write against a layout tree that persists
   (matching the shape every comparator's layout/render tree already has)
   than against one rebuilt from scratch every pump; and structured
   tree-diffing for `ui.lastChange` (§3.3 #11) is nearly free once node
   identity survives a pump, versus requiring its own diff pass if it
   doesn't.

### 4.3 What does NOT need to change

- **Identity scheme (ADR-021, `IdHash`)** — unaffected; folding scope+key
  hashes works identically whether or not the tree the identity addresses
  survives across pumps.
- **The signal/scope/memo API surface** — unaffected; `cx.signal`/`cx.scope`
  keep their exact current signatures.
- **`ui.getDeps`/`ui.whatDependsOn`** — unaffected in shape; they get
  *more* useful once node handles are stable across pumps (§5 below).

**ADR impact: none require reversal.** ADR-007's "no VDOM/diffing" clause is
satisfied by the Leptos precedent (retention without diffing); the campaign's
own M-B milestone (`ID0`-`ID2`, introducing `NodeHandle`/`nx-` ids) is
already-scoped prep work for exactly this move — "identity before
persistence" is already the campaign's own stated ordering constraint,
confirmed against source (`app.rs`'s generational freelist in `tree.rs`
`alloc`/`dealloc` is real, and `Tree::new()`'s per-pump discard is why
recycling never currently fires). A single new ADR should record the
decision once made (there is no existing ADR number reserved for it; the
next available slot is ADR-022), not because anything is being reversed, but
because "the `Tree`/`LayoutTree` are retained across pumps" is a real,
citable architectural commitment future contributors need a single place to
find — exactly the discipline the project already applies to every other
resolved question in `07-decision-log.md`.

---

## 5. Identity and the tree relationships

The 2026-08 review's F4 finding stands and is corroborated independently
here: `Tree` (SoA hit-test/paint source of truth) and `LayoutTree` (taffy
wrapper) are built in lockstep during one pass and reconciled by an explicit
copy loop keyed by a hand-maintained `built: Vec<(NodeIndex, LayoutNode)>`
correspondence vector (`app.rs:2579-2599`). The F3.4 patch path
(`patch_bg_bindings`) bypasses this loop entirely and must independently
guarantee it never touches size-affecting properties — two code paths with
one invariant between them, enforced by discipline, not by the type system.

**The A+ design is a derived projection, not a third hand-synced structure.**
The retention work in §4 is the actual fix here too, for a reason worth
stating precisely: once `Tree` and `LayoutTree` both persist across pumps
(rather than being rebuilt into a fresh `built` vector every time), the
correspondence between them stops being "a vector populated once per rebuild
that must be exactly right" and becomes "a stable mapping that is
established once, when a node is first created, and never needs
re-establishing" — the bijection becomes a structural invariant of
node-creation (one `NodeIndex` ↔ one `LayoutNode`, allocated together, freed
together) rather than a per-rebuild bookkeeping obligation. The semantics
tree (`SemanticsNode`) and the a11y tree (`accesskit::TreeUpdate`) are
already *derived* from `Tree`/`NodeMeta` (per the review's own system-model
diagram — `SEM["semantics_doc()"] --> A11Y`), which is the right shape;
extending that same "derive, don't hand-sync" discipline to the
`Tree`↔`LayoutTree` pair itself, by making the pairing structural rather
than a per-rebuild vector, is the fix. This is not new engineering beyond
what §4 already scopes — it is the same retention work, described from the
identity-invariant angle rather than the performance angle.

**In the interim (before §4 lands), the cheap mitigation the review already
named should ship regardless:** a debug-assertion coherence pass — every
live `Tree` node has a corresponding `LayoutTree` entry after each
rebuild/patch — gated into CI, converting a silent-drift bug class into a
loud test failure. `Tree::bounds` correctly returns `Rect::ZERO` for a
missing index rather than panicking (crash-safety), but that's the wrong
choice for *surfacing* this exact bug class during development; the debug
assertion is the complement, not a replacement.

---

## 6. Error handling and safety

`catch_unwind` + `AssertUnwindSafe` is used as control flow twice
(`app.rs:2386-2387`'s whole-app `rebuild` catch, `boundary.rs:16`'s subtree
`error_boundary`), suppressing the compiler's own `UnwindSafe` check rather
than proving safety. `RefCell` (not `Mutex`) means no poisoning/deadlock,
but nothing enforces that signal mutations within one scope run are atomic
— a panic mid-multi-write leaves partial state that both boundaries happily
continue running against.

**Verdict: panic-as-control-flow is not disqualifying for A+, but the
current documentation overclaims what it guarantees, and that gap is worth
closing independently of any redesign.** Two concrete, cheap fixes, neither
requiring "errors as values" (a much larger authoring-API change with its
own costs, not recommended given the audience is Rust widget authors who
already accept `Result`/panic as Rust's normal idiom):

1. **State the actual guarantee precisely** in `error_boundary`'s docs: "the
   *tree* is guaranteed consistent after a caught panic; application state
   may be partially mutated up to the panic point" — a one-line doc fix,
   ships immediately.
2. **Add transactional semantics scoped to the panic-recovery path
   specifically** — a generation-stamped rollback of writes since the last
   successful rebuild/pump boundary, reusing the existing `batch`
   infrastructure's write-coalescing machinery (`state.rs:738`) as the
   substrate rather than building a new one. This is genuinely optional for
   A+ (the risk is low-probability per the review's own risk register) but
   cheap once `batch` is being touched for other reasons (the RD-series
   post-mortem already found and fixed a real `batch_depth` leak-on-unwind
   bug in this exact area, so the machinery is already receiving
   maintenance attention).

**`unsafe` survey.** 13 `unsafe` blocks total across the entire workspace,
independently counted: `lumen-core/src/tasks.rs:409` (one `Waker::from_raw`
for a custom executor vtable), `lumen-render/src/gpu.rs` ×3 (byte-casting
typed slices for wgpu buffer uploads — the standard, unavoidable pattern for
any GPU API binding), `lumen-shell/src/lib.rs` ×1 and
`lumen-shell-android/src/imp.rs` ×2 (NativeActivity/JNI FFI glue, inherent
to the platform), `lumen-cli/src/hotpatch.rs` ×6 (all `libloading` calls,
already documented per-call with the specific safety argument — "the
fixture cdylibs expose only plain `extern "C"` functions with no global
ctors"). This is a small, unremarkable footprint for a GUI framework with a
GPU backend and native mobile shells — none of it is a novel safety risk
beyond what any Rust project doing the same class of FFI/GPU work would
carry, and none of it bears on the architecture questions this report is
scoped to.

---

## 7. Cross-platform

Desktop is the primary shape in practice: `lumen-shell` is 1,929 lines vs.
362 (Android `imp.rs`) + 17 (Android `lib.rs`) + 136 (iOS) + 230 (web) —
independently re-measured, matching the 2026-08 review's figures exactly.
No `trait Platform`/`trait Shell` exists across all four; platform
differences are threaded through shared crates via `#[cfg(...)]` rather than
one abstraction boundary each shell implements against (F7 in the review).

This folds into §1's modularity work as axis 8 (shell/platform), and
`02-api-modularity-path.md` §5 has already designed and costed the specific,
scoped fix: a `lumen-shell-core` crate capturing the genuinely-shared
23-line `render_into`/session/pointer-translation logic currently
byte-identical-duplicated between iOS and web (Android's blit path is
correctly *not* forced into the shared crate — it's a stride-aware,
safe-area-offset native-window write with no equivalent shape on the other
two platforms). **Recommendation: adopt that design as-is** rather than
building a heavier `trait Platform` — per Slint's own precedent (§1.2), not
every axis needs a runtime trait; a shared helper crate is the right weight
for "three shells duplicate the same 23 lines," and a trait becomes
justified only if a fourth platform's needs diverge enough to need dynamic
dispatch over shell implementations at runtime, which nothing today
requires.

---

## 8. Missing seams for 1.0

Independently spot-checked against source (not merely re-quoting the
review): `crates/lumen-widgets/src/i18n.rs` (230 lines, real Fluent-style
catalogs — confirmed present), `crates/lumen-render/src/media.rs` (confirmed
— `VideoSource` trait exists, only implementation is `TestPattern`, a
procedural gradient; the doc comment itself says hardware decode "tracked
separately"), `rg -il plugin crates/*/src` (confirmed — matches only CLI
subcommand help text, no widget-registration/extension-point API),
`WindowDesc`/`App::window()` (confirmed present and real, `app.rs:113`).

| Capability | Status | Seam exists? |
|---|---|---|
| i18n (catalogs, plurals, locale numbers) | Present, real | — |
| RTL layout mirroring | Present, real (`mirror_rtl`) | — |
| RTL/bidi text *shaping* (mixed-direction runs) | Unverified, no golden test found | Depends on parley's own bidi handling |
| Multi-line / rich text editing | Partial — caret/selection/undo/multiline; no *styled* multi-run editing | — |
| Video/media playback | Stub — deterministic test pattern only, no real decode | `VideoSource` trait is the seam; no real backend implements it |
| Plugin/third-party widget extension | Missing for widgets-as-a-registry (though `LeafWidget` already makes ad hoc third-party widgets first-class); missing entirely for style properties until `register_property` ships | `LeafWidget` is a real seam today; style has none until §1.3's item ships |
| Runtime theming | Present — `set_stylesheet` reuses the Tier-1 reload path | — |
| Multi-window | Present, real | — |
| Drag-and-drop (in-app, OS-level) | Present | — |
| Virtualized lists | Present, real, load-tested to 1M rows | — |
| Animation composition (sequencing/interruption/physics) | Basic only — `spring()` primitive exists, no choreography API | No seam yet |
| Accessibility beyond basics | Structurally sound (exhaustive role match); live AT verification sandbox-blocked | Structural seam exists; verification doesn't |

**Net for architecture specifically:** the axes with *no seam at all* are
narrower than the review's framing suggests once "does a trait/extension
point exist" is separated from "is it fully implemented" — `LeafWidget`,
`VideoSource`, and `set_stylesheet` are all real seams with thin
implementations behind them (a resourcing question, not an architecture
gap). The two genuine seam-shaped gaps are style-property extensibility
(closes with `register_property`, already designed) and animation
choreography (no design exists yet anywhere in the project's docs — flagged
here as the one item in this table that would need its own design pass
before costing, not because it's hard, but because nothing has scoped it).

---

## 9. Is the "AI-first GUI framework" thesis defensible at A+?

**Direct answer: yes, but the moat is a committed architectural stance, not
a technical secret — and staying ahead requires treating that stance as
load-bearing under every future change, including the modularity work this
document recommends.**

The separability question the brief asks is answerable precisely by reading
the dependency graph: `crates/lumen-agent/Cargo.toml` depends directly on
`lumen-core` **and `lumen-widgets`** — `lumen-agent::handle()` calls into
`Headless<R,E>`, the *same* struct `App`'s live rendering loop uses, not a
serialized snapshot or a re-derived view. This is why ADR-009 ("semantic
tree = a11y tree = locator tree = agent tree") holds *in code*, not just in
docs — but it is also why the observability layer is **not** a bolt-on
library another framework could adopt by adding a dependency. It requires
the host framework to expose its live internal tree/state structures
directly, read-only, to an out-of-process caller — which means any
framework wanting Lumen's exact guarantee has to make the same up-front
commitment Lumen made: no private internal representation the renderer sees
that the introspection layer doesn't.

**That commitment is copyable in principle and has not been copied in
practice.** Xilem's Masonry widget tree is explicitly designed to be
introspectable ("you can inspect that widget tree at runtime... and
generally have an easier time debugging," per Masonry's own README) — the
*capability* to build a Lumen-shaped agent surface on top of a retained
widget tree clearly exists elsewhere in the Rust GUI ecosystem. No
competitor examined in the sibling competitive study (Flutter, Compose,
SwiftUI, Qt, GTK4, egui, Slint, Dioxus, Makepad, Avalonia, Iced) has built
one. That is a real, current, defensible lead — but "no competitor has done
this yet" is a different and weaker claim than "no competitor could."

**What makes the thesis defensible at A+, concretely, rather than merely
true today:**

1. **Every future modularity axis (§1) must preserve, not merely tolerate,
   observability.** The state-store recommendation in §1.3 is explicitly
   scoped to avoid genericizing away the exact-attribution property
   `ui.getDeps` depends on — this is the test every future "make X
   swappable" proposal needs to pass. A `LayoutEngine`/`TextEngine` swap
   that doesn't also expose its reasoning to `ui.explain` (§3.3) would
   quietly reopen the "the agent sees a reconstruction, not the truth" gap
   the architecture currently avoids by construction.
2. **The retained-tree work (§4) strengthens the claim rather than
   threatening it**, contrary to the campaign's own stated worry — stable
   node handles across pumps make `node-<index>` (soon `nx-<hash>`, per the
   campaign's own ID-series) a durable identity an agent can hold across
   multiple interactions, not a per-pump-only handle. Retention is a
   precondition for a *stronger* observability guarantee, not a competing
   concern.
3. **The lean/full feature-matrix gap (§3.3 item #10) is the nearest-term
   threat to the thesis being true in practice**, not in principle: if the
   framework's recommended default ships with zero agent introspection
   (today's `snapshot`-gated status quo), then "AI-first" is true only of
   dev builds, which is a materially weaker claim than the marketing
   implies. Fixing this (a distinct "agent" point on the feature matrix) is
   cheap and should not wait for a broader modularity pass.

**On "peak performance" and "fast iteration" as the other two pillars:**
this document doesn't re-litigate the performance verdict (that's the
competitive-benchmark study's job under the new definitions), but notes
that §4's retained-tree recommendation is where performance and
observability stop being separate investments — the same piece of work
closes the reactivity pessimization *and* makes tree-diff observability
(§3.3 #11) nearly free *and* gives modularity's `LayoutEngine` axis a
natural target to abstract over. That convergence is itself evidence the
three-pillar framing was never really three independent bets — it's one
architectural commitment (a real retained, introspectable, patchable tree)
that performance, observability, and modularity all cash out against
differently. And §2's measured build-time baseline adds a fourth: "fast
iteration" was never actually bottlenecked on hot-reload machinery at all —
it was already delivered by Rust's ordinary incremental compiler, a fact
the original three-pillar framing didn't have the measurement to know.

---

## 10. Blocker analysis

| Area | Blocker | Class |
|---|---|---|
| Modularity: renderer, executor | none — already done | done |
| Modularity: layout engine | trait extraction from an already-isolated wrapper | **unfinished** |
| Modularity: text engine | encapsulation (doesn't exist yet) + trait extraction | **unfinished**, larger scope than layout |
| Modularity: style engine | `register_property` registry | **unfinished**, already designed elsewhere |
| Modularity: state store | full swap threatens the agent-observability contract | **irreducible in the short term** as posed; **revisitable** at the narrower `SlotStorage` scope recommended in §1.3 |
| Modularity: allocator | no lever exists yet | **unfinished**, cheap (docs + recipe, not new API) |
| Modularity: shell/platform | no shared crate yet | **unfinished**, already designed elsewhere |
| Hot reload Tier 2, capability ③ (full instance migration) | no precise pointer map in any AOT/non-managed runtime | **irreducible** — a gap of kind, confirmed independently by this pass (Rust's own unspecified cross-compilation layout guarantee is the specific reason), not merely under-engineered |
| Hot reload Tier 2, capabilities ①/② (code swap + coarse state retention) | genuinely reachable and shipping elsewhere in the Rust GUI ecosystem (iced, Bevy) — but Lumen's own measured build-time baseline (§2.3) shows the speed problem it would solve doesn't currently exist | **revisitable**, deliberately and now evidentially deferred, not merely deprioritized by fiat |
| Hot reload Tier 2's ABI-hash claim | hardcoded literal, claims a property that is unsound in category (no hash can fingerprint what Rust doesn't guarantee is stable) | **unfinished** — cheap, should ship regardless of priority (it's a live-UB risk, not a feature gap) |
| Reactivity pessimization | discard-and-rebuild target under a sound reactive core | **revisitable** — the CUT decision, re-openable without an ADR reversal |
| Tree/LayoutTree hand-sync | same root cause as reactivity pessimization | **revisitable**, same fix |
| Observability: 11 blind spots | each individually cheap; no unifying primitive existed | **unfinished** — the `ui.explain`/`getInteractionState` design closes this class |
| `catch_unwind` transactional guarantee | documentation overclaims; real fix optional | **unfinished**, low priority, cheap |
| Cross-platform shell parity | no shared crate | **unfinished**, already designed elsewhere |
| Missing seams: style extensibility | no registry | **unfinished**, already designed |
| Missing seams: animation choreography | no design exists at all | **unfinished**, needs a design pass before costing |
| Missing seams: video/media, RTL text shaping verification | thin implementation behind a real seam, or unverified | **unfinished**, resourcing not architecture |
| AI-first thesis's durability | requires every future modularity/perf change to preserve exact-attribution observability | **revisitable discipline**, not a one-time fix — an ongoing constraint on §1/§4's execution |

**Zero items in this table are irreducible in the sense of "blocks A+
outright."** The two genuinely irreducible items (full-instance-migration
hot reload, and a fully-generic state store that doesn't compromise
observability) are both irreducible *as stated*, and both have a narrower,
reachable restatement (coarse code-swap hot reload, now evidentially
low-value given §2.3's measurement; a scoped constrained-memory store
variant) that satisfies what `DEFINITIONS.md` actually asks for.

---

## 11. The path — ordered, costed, ADR reversals marked

**Headline finding: this path requires zero ADR reversals.** Every
recommendation above is additive — a new trait, a new ADR recording a
decision none of the existing 21 forecloses, or finishing already-scoped
work. Where a new ADR is warranted (the retained-tree commitment, §4.3), it
is a fresh entry at the next available slot (ADR-022), not a revision to an
existing one.

### Phase 0 — cheap, parallel, no prerequisites (~1 week)
- Fix `hotpatch.rs`'s ABI-hash claim: remove the pretense, always downgrade
  to Tier 3 (§2.5 item 1) — ~1 hour.
- Debug-assertion `Tree`↔`LayoutTree` coherence pass, CI-gated (§5) — cheap,
  mechanical.
- `error_boundary` diagnostic wiring (§3.3 item #8) — ~1 day.
- `catch_unwind` guarantee documentation fix (§6 item 1) — ~1 hour.

### Phase 1 — reactivity target fix (~2-3 weeks, sequenced per the campaign's own CP0→CP1 gate)
- CP0/CP1 (campaign-scoped, unchanged).
- CP2.3 promoted forward (§4.2) — the deciding measurement.
- A.3.3-scoped retention increment (§4.2/§4.3) — the CUT item, re-opened, right-sized. **New ADR-022**, recording the decision once CP2.3 confirms it (predicted outcome per the cross-framework evidence, not asserted outright here).

### Phase 2 — modularity core (~3-4 weeks, mostly parallel with Phase 1 after retention lands for layout specifically)
- `PlatformConfig` bundle + `App<R,E,P>` (§1.2) — depends on nothing, can start immediately, but `LayoutEngine`'s default impl is more natural post-retention.
- `trait LayoutEngine` (§1.3) — sequenced after Phase 1's retention work.
- `lumen-text` encapsulation + `trait TextEngine` (§1.3) — independent, can run in parallel.
- `register_property` (§1.3, already designed in `02-api-modularity-path.md`) — independent, parallel.
- `SlotStorage` scoped state-store variant (§1.3) — independent, parallel.
- Allocator recipe (§1.3) — independent, cheap, parallel.
- `lumen-shell-core` (§1.3/§7, already designed) — independent, parallel.

### Phase 3 — observability unification (~4-5 weeks, parallel with Phase 2, touches different files)
- `ui.explain` (four kinds) (§3.3) — the main piece.
- `ui.getInteractionState` (§3.3) — small.
- `ui.getAccessKitTree` (§3.3) — small.
- Agent-flavored feature-matrix point (§3.3 item #10) — depends on Phase 2's `PlatformConfig` landing first.
- Structured tree-diff for `ui.lastChange` (§3.3 #11) — cheaper if it lands after Phase 1's retention work; can proceed independently but at higher cost if sequenced first.

### Phase 4 — the items with no design yet, sized only after a design pass
- Animation choreography seam (§8) — needs a design spike before costing, not scoped further here.
- `filter`/`z-index`/`transform` hit-test parity, RTL mixed-direction text shaping verification — resourcing items per `02-api-modularity-path.md`'s own Phase 5, orthogonal to this document's architecture recommendations.

### Explicitly not scheduled
- **A real Tier-2 hot-patch integration** (§2.5 item 2). Per `DEFINITIONS.md`'s demotion and §2.3's measured baseline, this is deliberately left off the path. If priorities change, iced's shipped `debug::hot()` pattern (~60-100 lines against Lumen's own single `build(cx) -> Element` call site) is the proven template, at the cost of a real dependency on either the Dioxus CLI or the still-experimental standalone `cargo-hot` build server.

### Total

| Phase | Calendar, sequential | Parallelizable? |
|---|---|---|
| 0 | ~1 week | yes, fully |
| 1 | ~2-3 weeks | partially (CP0/CP1 first; CP2.3 gates the rest) |
| 2 | ~3-4 weeks | mostly (layout trait waits on Phase 1) |
| 3 | ~4-5 weeks | mostly (runs alongside Phase 2) |
| **Total, 1 engineer** | **~10-13 weeks** | |
| **Total, 2-3 engineers on independent tracks** | **~6-8 weeks** | |

This is the same order of magnitude as the sibling consumer-API/modularity
document's own ~7-9 week estimate for its scope, and — per that document's
own framing — layers onto, rather than replaces, the already-approved
campaign's M-A→M-F performance/observability work. **No item in this path
requires reversing an architectural decision already made; the work is
entirely in the category the campaign itself named as what it declined to
do to reach A+.**
