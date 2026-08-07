# What A+ means — authoritative definitions

Set by the project owner, 2026-08-07. **These supersede the working definitions
used in `00-what-is-a-plus.md` and the four path documents**, which were written
against an assumed reading. Any future research or planning must be measured
against the bars below, not against the earlier ones.

## Performance

Three concrete axes:

1. **Reaction latency** — how fast the UI responds to input.
2. **Full-view build time** — how fast a complete view can be built.
3. **Node capacity** — how many nodes the UI can hold while staying performant.

**A = matches the current industry leader. A+ = surpasses it.**

Consequence: the bar is *relative and external*, so it is undefined without
competitive measurement. This makes the ~5-person-day competitive benchmark
(egui, Slint, GTK4 — all runnable on this box, no new toolchains) a
**prerequisite for grading**, not the optional extra the campaign treated it as.

## Modularity

**A+ = practically every important part of the framework's implementation can be
swapped out.**

Not "well-organized crates" — *substitutable internals*. The grading question
becomes: for each major subsystem, can a third party replace it without forking?

Current state of the axes:

| Subsystem | Swappable today? |
|---|---|
| Renderer | **Yes** — `App<R = DefaultRenderer, …>` (`app.rs:63`), `pub trait Renderer` (`lumen-render/src/lib.rs:69`), two working impls |
| Executor / task spawner | **Yes** — `E = InlineSpawner`, `pub trait Spawner` (`lumen-core/src/tasks.rs:158`) |
| Layout engine | **No** — taffy is behind a wrapper crate (nothing outside `lumen-layout` sees a taffy type) but there is no trait to implement |
| Text/shaping engine | **No** — parley/swash are concrete |
| State store | **No** — `Runtime` is concrete; `ReadCx`/`WriteCx` traits exist but the store does not |
| Style engine | **No** — `Style::apply` is a closed `match`; no property registration |
| Allocator / memory strategy | **No** |
| Shell / platform | Partially — separate crates, but no shared trait |

**Two of eight axes are done, and they establish the pattern.** A+ modularity is
an extension of an existing, working design — not a redesign.

## Resource usage

**A+ = modular enough to swap internals to meet essentially every use case** —
e.g. generically define the application struct to target heavily
resource-constrained equipment, while **defaulting** to a full-power
configuration that delivers A+ performance. **And** beat the competition on
resource use.

Two distinct requirements, both of which must hold:

1. **Configurable floor** — a constrained profile (small memory, no GPU, possibly
   no OS) reachable by type parameters and features, not by forking.
2. **Competitive default** — the full-power default still wins on resource use
   against the frameworks being compared.

Consequence: **resource usage is now downstream of modularity.** It is graded on
the *span* of configurations reachable from one codebase, not solely on the
default binary's size. The `<5 MB` figure becomes one point on a curve rather
than the target.

Closest prior art to study: **Slint** (one codebase spanning MCU software
rendering to desktop GPU), embedded Rust HAL traits, and `wgpu`'s own backend
abstraction.

## Architecture

**A+ = the architecture that enables all of the above, while giving agents proper
observability and the ability to develop applications against the framework with
reasonable iteration cycles.**

**Hot-reload speed is a plus, but not the most important thing.**

Consequence — and this is significant: `00-what-is-a-plus.md` identified exactly
one **gap of kind** (Tier-2 hot reload matching Flutter/Compose seamlessness,
judged architecturally unclosable by any Rust AOT framework). **That gap is now
off the critical path.** Every remaining gap the competitive study found is a gap
of *degree*.

"Reasonable iteration cycles" is the real bar — which points at build times,
test/verify latency, and the tier-1 (`.lss`/asset) reload that already works,
rather than at sub-second Rust code patching.

## Consumer API

**Not redefined by the owner.** Continue using the working definition from
`02-api-modularity-path.md` — an API an agent can use correctly without docs,
where failure modes surface as errors rather than silence — until told otherwise.

## What these definitions change

1. **Modularity becomes the spine.** A+ resource usage is *defined in terms of*
   modularity; A+ architecture is defined as enabling it. Modularity is no longer
   one of five parallel dimensions — it is the load-bearing one.
2. **The one unclosable gap stops mattering.** Hot-reload seamlessness is
   demoted; nothing else is a gap of kind.
3. **Competitive benchmarking becomes a prerequisite.** "Match/surpass the
   leader" cannot be graded without it. The cheap tier is ~5 person-days.
4. **Binary size is re-framed** from a fixed target to the low end of a
   configurable span.
5. **The existing `App<R, E>` generic seam is the foundation**, not a curiosity —
   the work is extending it to the remaining six axes.
