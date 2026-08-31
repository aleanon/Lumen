# Plan: the Element deletion (staged) — E0–E5 (started 2026-08-31)

The 1.0 authoring break MUT7b staged. Goal: no `Element` in authoring code,
then no `Element` in the engine. 208 files return `Element` at the start
(inventory 2026-08-31): 59 lumen-widgets tests, 43 lumen-widgets src (widget
`build` fns), ~65 example files, 11 lumen-agent tests, 3 lumen-app engine
files, the rest scattered.

## The three migration forms (the recipe)

1. **Full form** — app state is one `#[derive(Reactive)]` struct
   (`App::with_state`), views are statement-form (`Stack::column(|c| …)`)
   with typed widgets. Exemplar: `examples/hello`.
2. **Mixed form** — statement-form root; cx-coupled helpers (widgets that own
   keyed view-local state: `switch`, `select`, `tabs`, third-party fns) are
   built eagerly and MOVED into the `FnOnce` body as `Element` children (the
   D1 boundary, preserved). Exemplar: `examples/gallery`.
3. **Signature-only** — where a public `-> Element` contract is pinned by a
   consumer (the Android/iOS shells' `run_styled(build)`), the body migrates
   later with the consumer.

## API gaps found and closed in E1

- Facade: `Button`, `Label`, `Stack`, `Kids`, `Reactive` were unreachable
  from `lumen` (ADR-W2 makes scaffolded apps facade-only). Exported; gated by
  `facade_complete::statement_form_and_state_struct_are_facade_complete`.
- `#[derive(Reactive)]` emitted `::lumen_core::…` paths — uncompilable in a
  facade app. Now serde-style: default root `::lumen`; framework-internal
  call sites declare `#[reactive(crate = "lumen_core")]`.
- `Stack` bodies were `FnMut`; eagerly-built children could not be moved in.
  Now `FnOnce` (adapted through an `Option` to the object-safe
  `write_body`), which is what makes the mixed form writable.
- (MUT7b, earlier) `Stack::width`/`height` for the definite containing block.

## Stages

- **E0 ☑** this plan; recipe; inventory.
- **E1 ☑** scaffold (`lumen new` emits the full form — new apps start
  Element-free), facade + derive fixes, exemplars `hello` (full) and
  `gallery` (mixed). `hello`'s golden re-blessed deliberately: typed
  `Button` is the canonical look; `Element::button` was the legacy helper.
- **E2 ◐** remaining example crates. **E2a ☑ (2026-08-31)**: ten
  single-widget demo crates removed — each showed one widget the
  `widget_showcase` catalog already seeds (accordion, toast, progress_bar,
  loading_spinners, markdown, image, datagrid, pane_grid, modal, svg).
  `iced-parity` stays (it mirrors iced's shapes deliberately);
  `widget_showcase` itself defers to E4 (its catalog builders are
  `fn(..) -> Element` by design). Signature-pinned: `settings` (Android
  shell) migrates with E4's shell work.
- **E3 ◻** tests: lumen-widgets (59) + lumen-agent (11) + shells (2).
- **E4 ◻** widget internals: replace the `@direct_bridge` `build → Element`
  with native `lower` per widget (43 files) — this is where the transient
  per-node `Element` dies. Convert `run_styled`-class shell entry points to
  `impl Direct`.
- **E5 ◻** engine: `build_node` consumes widgets, `Element` deleted, the
  type_sizes gate retired. R8-costed ~5% of a changed frame.

## Verification per tranche

Each migrated example keeps its tests green; a deliberate pixel change (legacy
helper → canonical typed widget) re-blesses its golden **in the same commit
with the rationale**; the workspace suite + facade gate + scaffold
compile-check (`lumen new` into scratch + `cargo build`) close each stage.
