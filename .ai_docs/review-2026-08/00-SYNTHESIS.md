# Lumen — five-domain review, 2026-08-06

Five independent reviewers, disjoint scopes, no shared drafts. Each was told the
`.ai_docs/02–05` specs had drifted ~30% from code and that `06-task-graph.md`
checkmarks are unreliable, and each was required to ground every claim in source
it read itself.

| # | Report | Grade |
|---|---|---|
| 1 | [Performance](01-performance.md) | **D+** |
| 2 | [Consumer API](02-consumer-api.md) | **C+** (B- on "easier for an AI than iced/egui") |
| 3 | [Modularity](03-modularity.md) | **B-** |
| 4 | [Resource usage](04-resource-usage.md) | **C+** desktop / **D** mobile |
| 5 | [Architecture](05-architecture.md) | **B-** |

## The one-line result

The **AI-first thesis is architecturally sound and is the project's real asset**;
the **"peak performance" claim is not currently supported by the code**, and the
gap between what the docs assert and what the code does is wide enough to be the
single biggest risk to the project.

## What is genuinely good

These are load-bearing and should not be traded away in any refactor.

1. **Observability is architectural, not bolted on.** `lumen-agent` and
   `lumen-test` are thin callers into the *same* `Headless<R,E>` the renderer
   paints from (`crates/lumen-widgets/src/app.rs`). The agent cannot observe a
   tree that disagrees with the screen — that holds by construction. This is the
   differentiator, and it survived adversarial review.
2. **No `Message` enum.** `App::new(impl Fn(&mut BuildCx) -> Element)` removes
   the single largest source of LLM error when writing iced-style UIs.
3. **Honest self-measurement.** The project's own `nodecost.rs`/`identity.rs`
   benches are rigorous and were used to *falsify the project's own thesis*
   (`docs/results-node-cost-n0.md`). Two reviewers noted this as a strong signal
   about engineering culture.
4. **Cycle-free core layering.** `lumen-core → lumen-render/lumen-layout →
   lumen-text/lumen-style → lumen-widgets` verified acyclic; the `Renderer` trait
   is a real generic abstraction with two working backends.
5. **Macro diagnostics are excellent** where they exist.

## Cross-cutting theme 1 — claim vs. reality

Every reviewer independently hit this. It is the dominant finding of the review.

| Claim | Reality | Source |
|---|---|---|
| `<5 MB` hello-world (`01-architecture.md:70`) | 22.1–22.5 MB measured; lean profile 7.5 MB still misses | 04 |
| Damage tracking reduces GPU work | Damage computed, then discarded — full re-encode every frame | 01 |
| `cx.scope` memoization is the incremental path | 1.44× slower, +85% allocations vs full rebuild | 01, 05 |
| Tier-2 hot reload gated by ABI hash | Hash is the literal `0x1111_2222_3333_4444` | 05 |
| Typed-widget migration complete | Legacy duplicates still maintained, with 2 shipped regressions | 02 |
| Lean `--no-default-features` profile works | Never compiled by CI as a whole | 03, 04 |
| Pure Rust | `ldd` shows real links to libgtk-3, libglib-2.0, libdbus-1 | 04 |

`AGENT.md` carries a *binding* same-commit doc-currency rule. It is being
violated, and the Consumer API review found four fresh instances in
`.claude/skills/` alone. The 2026-07 remediation appears to have re-synced the
documentation without closing the underlying behavioural gaps.

## Cross-cutting theme 2 — silent failure

The systemic defect, and the one most corrosive to the stated audience. An agent
that cannot see the screen depends entirely on errors surfacing.

- **39 of 89 `.lss` properties parse and then silently do nothing.** No warning.
- **21-entry silent-failure inventory** in the Consumer API report.
- **`cx.signal` keys have no compile-time type link** — same key, different type
  yields silent state aliasing or an unrelated-looking panic. Found independently
  by the API and Architecture reviewers.
- **Mobile memory-pressure callbacks are absent** — not stubbed, never wired.
- **A hardcoded ABI hash** that reads as a safety gate and enforces nothing.

## Cross-cutting theme 3 — untested invariants

Things believed true that nothing verifies:

- the lean feature profile (never compiled in-workspace — Cargo unifies features
  across all ~70 members, and 46 of 51 examples request full defaults);
- benchmark regressions (the good benches are not CI-gated);
- mobile artifacts (no APK or IPA has ever been built in this repo);
- the `<5 MB` size target (gated only by a throwaway out-of-workspace crate).

## Independent corroborations

Where two reviewers reached the same conclusion from different directions —
these carry the most weight:

- **Memoization is a pessimization** — Performance (mechanism: `copy_node`/
  `copy_span` at `app.rs:2754-2850` mint fresh tree nodes, fresh taffy nodes and
  4 HashMap remove+insert pairs *per memo hit*) and Architecture (measurement).
- **State-key type confusion** — Consumer API (ergonomics) and Architecture
  (identity model).
- **Lean profile is unverified** — Modularity (Cargo unification) and Resource
  usage (measured binary sizes).

## Recommended order of work

Ranked by impact ÷ effort. The first four are cheap relative to their payoff.

1. **Wire damage into the GPU present path.** `app.rs:4206-4209`,
   `gpu.rs:1027-1106`. The machinery already exists and is computed every frame;
   only the consumer is missing. Highest-leverage change in the review.
2. **Stop shipping a 15.5 MB CJK font by default.** `GoNotoKurrent-Regular.ttf`
   is 15.5 MB of a 22 MB binary. Subset it or feature-gate it; the Latin face is
   already 355 KB.
3. **Make silent `.lss` no-ops loud.** A property the runtime ignores must warn
   or error. This is the AI-first thesis defending itself.
4. **Decide `cx.scope`'s fate.** It is currently a measured net loss — either fix
   the copy semantics or delete it. Shipping a slow cache is worse than no cache.
5. **Cap the unbounded caches.** `asset.rs:17-19` (decoded images, no cap, no
   `clear()`), `editor.rs:28-29` (undo clones the whole buffer per keystroke —
   worst-case quadratic).
6. **Make the ABI hash real, or make tier 2 fail loudly.** A placeholder gate
   will load a mismatched library and hand you memory corruption.
7. **CI-gate the three untested invariants**: a lean-profile build job, bench
   regression thresholds, and the size gate.
8. **Finish or delete the duplicate legacy widget functions** — they have already
   drifted into shipped regressions (stale slider format, inert menu).
9. **Split `lumen-widgets`.** 26k LOC containing the widget catalog, the entire
   headless app runtime (`app.rs`, 4,613 lines), an app toolkit, and a11y
   tooling. Also retire the `widgets_m1`/`widgets_m3` milestone names from the
   public facade before 1.0 — they are a one-way door.
10. **Ship a virtualized list.** `Scrollable` lays out all children every frame,
    which disqualifies the performance claim on any list-driven app.

## Reading order

For the fastest orientation: **05 (architecture)** for the model and the
observability blind-spot list, then **01 (performance)** for the frame-pipeline
findings, then **02 §Silent-failure inventory**.
