# Lumen Handoff Package — READ THIS FIRST

You are an autonomous coding agent tasked with building **Lumen**, an AI-first, cross-platform GUI framework in Rust. This package is your complete specification. It is layered:

| Doc | Layer | Authority |
|---|---|---|
| `00-HANDOFF-README.md` | Operating rules for you, the agent | Binding |
| `01-architecture.md` | Vision, architecture, platform strategy | Binding unless it conflicts with 02–05 |
| `02-spec-core.md` | Core API contracts (tree, widgets, signals, state, events) | **Binding, normative** |
| `03-spec-semantics-agent.md` | Semantic tree schema + agent/dev-server protocols | **Binding, normative** |
| `04-spec-lss-styling.md` | The `.lss` styling language | **Binding, normative** |
| `05-spec-testing.md` | `lumen-test` harness API and snapshot rules | **Binding, normative** |
| `06-task-graph.md` | Ordered tasks with acceptance criteria | Your work queue |
| `07-decision-log.md` | Resolved decisions (ADRs) + escalation list | Binding |

Reading order: 00 → 06 (skim) → 02 → 03 → then 04/05 as their tasks come up. 01 is context.

> **Normativity qualifier (2026-07-09).** Docs 02–05 were re-grounded
> against the implementation after the docs↔code audit
> (`docs/review-docs-vs-code-2026-07.md`): sections describe what *exists*;
> anything not yet built is explicitly marked **planned** with its task in
> `docs/plan-remediation-2026-07.md` (the current work queue, alongside 06).
> 06's checkboxes now use ◐/✗ for partial/absent. When implementing, trust a
> *planned* marker over the surrounding prose; when a doc and the code
> disagree, the code is the bug in behavior, the doc is the bug in claim —
> fix per the doc-currency rule in `AGENT.md`.

## Operating rules

1. **Contracts are law.** The public APIs, schemas, grammars, and wire protocols in docs 02–05 are normative. Do not rename, restructure, or "improve" them while implementing. If a contract is genuinely broken (won't compile, internally contradictory, unsound), fix it minimally, and record the change in `07-decision-log.md` under "Agent amendments" with rationale, in the same commit.
2. **Work the task graph in order.** Tasks in `06-task-graph.md` are topologically sorted; dependencies are explicit. Do not start a task whose dependencies' acceptance criteria don't pass. Do not reorder milestones — the order is deliberately *verification-first* (you build your own eyes before you build the body).
3. **Definition of done is executable.** A task is complete only when its listed acceptance commands exit 0 in CI. "It looks right" is never done. Every task adds tests; no PR may reduce coverage of public APIs.
4. **Decide-and-record, don't stall.** When you hit an unspecified detail: (a) check `07-decision-log.md`; (b) if absent and the decision is *local* (naming, internal structure, dependency patch-version), choose the most conventional option and record it in the log; (c) if the decision is *architectural* (public API shape, new dependency, file format, protocol change), it is on the escalation list in 07 — stop that task, leave a `BLOCKED.md` note in the repo root describing options and your recommendation, and move to the next unblocked task.
5. **Workspace hygiene.** One Cargo workspace, crate names per `02-spec-core.md` §1. `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace` must pass on every commit to `main`. MSRV: pin the stable toolchain in `rust-toolchain.toml` at repo init and record it in the decision log.
6. **Dependencies.** Only the dependencies whitelisted in `07-decision-log.md` ADR-003 plus their transitive closure. Adding any other runtime dependency is an escalation-list decision. Dev-dependencies for testing are at your discretion (record them).
7. **Performance gates.** Benchmarks in `06-task-graph.md` are regression gates from the moment they land. A PR that regresses a gated benchmark >10% must not merge; either fix it or escalate.
8. **Commit discipline.** One task → one PR/commit-series, message prefixed with the task ID (`[T0.4] CPU display-list renderer`). Each merge updates the task's checkbox in `06-task-graph.md`.
9. **Self-verification loop.** From task T0.9 onward you have a headless screenshot + semantic-tree harness. Use it: every widget/styling/rendering task must include at least one golden-image test and one semantic-tree assertion. When you are uncertain whether rendering is correct, render it and look at the PNG; do not guess from code.
10. **Docs are code.** Public items require rustdoc with at least one compiling example (`cargo test --doc` is part of done). Each crate keeps a `README.md` describing its contract surface.

## Environment assumptions
- Linux x86_64 dev host with stable Rust, plus CI runners for Windows and macOS (desktop targets are M0–M2; mobile toolchains arrive in M3 and have their own setup tasks).
- Headless rendering never requires a GPU or display server: the CPU backend (tiny-skia) is the reference renderer and runs everywhere, including CI.
- GPU tests are tagged `#[ignore]` by default and run on GPU-equipped runners via `cargo test -- --ignored --test-threads 1`.

## What success looks like (M0 exit, your first real checkpoint)
A binary `examples/hello` that renders a styled counter app; `lumen-test` drives it headlessly: queries the semantic tree, clicks the button by selector, asserts the label changed, and matches a golden screenshot — all in CI on Linux/Windows/macOS. Full criteria in `06-task-graph.md` §M0-exit.
