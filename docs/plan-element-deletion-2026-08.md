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
- **E2b, the unanimous finding of all four migration tranches:** alignment
  did not exist anywhere in the authoring surface — not on `Stack`, and not
  in `lumen_style` either, so neither inline css nor `.lss` could centre a
  card, which is the repo's universal example shape. Closed centrally:
  `Stack::align_items` / `justify_content` / `centered()` / `shadow()` /
  `grow()`.

## Pinned until E4/E5 (found by E2b, recorded rather than worked around)

- `App::window(desc, root)` stores `Rc<dyn Fn(&mut BuildCx) -> Element>` —
  secondary-window roots are signature-pinned (multi_window, counter).
- The **third-party widget ABI** is `fn(cx, ..) -> Element` (widget-rating
  exemplifies it) — it changes with E4's native-lowering story, not per crate.
- Typed containers (`Container::new`) still take `Vec<Element>`; statement
  bodies for typed widgets are E4 (widget_gallery's sections wait on it).
- `widgets::canvas` returns `Element` and is styled by field mutation — a
  typed statement-form `Canvas` would finish the clock/sierpinski/glass class.
- **`App::with_state` x P.3d (hazard):** a `.window(..)` tree gets its own
  runtime with NO installed state, so a generated `S::set_*` called from a
  secondary window panics. P.3d must decide whether the instance is shared or
  per-window; until then with_state apps and secondary windows do not mix
  (counter deliberately stays keyed for this reason, noted in its source).

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
- **E4 ✗ (2026-08-31)** widget internals: **closed on measurement.**
  Converting a widget to native lowering buys nothing — `Slider` converted
  properly measured 3 819 → 3 830 µs (zero), because the staging-tree cost
  is a working-set effect that needs ~500+ siblings to appear, and a
  widget's 2–48 internal children never reach it. The win it was chasing
  (11–14%) belongs to *containers* and is already shipped as `Stack` and
  adopted by the E2 examples. See the task-graph entry; bench arm
  `benches/src/bin/widgetlower.rs`. The shell entry-point conversion
  (`run_styled`-class, `impl Direct`) is unrelated to lowering cost and
  moves to E5 with the rest of the API change.
- **E5 ◻** engine: `build_node` consumes widgets, `Element` deleted, the
  type_sizes gate retired. R8-costed ~5% of a changed frame.

## Verification per tranche

Each migrated example keeps its tests green; a deliberate pixel change (legacy
helper → canonical typed widget) re-blesses its golden **in the same commit
with the rationale**; the workspace suite + facade gate + scaffold
compile-check (`lumen new` into scratch + `cargo build`) close each stage.
