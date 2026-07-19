---
name: building-apps
description: Use when creating a Lumen application, adding a screen or feature to one, or composing UI from the existing widget set (examples/*/src, or any app crate using lumen-widgets). Encodes the project shape (main_app convention, example-crate layout, just recipes), the real import style, the honest widget-availability catalog, state/signal rules, the app-level modules (forms/nav/i18n/undo/system/tasks), stable-id discipline, and the async/no-HTTP pattern.
---

# Building a Lumen app

An app is `fn main_app() -> App` — a pure build function plus a stylesheet —
packaged as a small crate that runs headless (tests, agent) and windowed
(`just run`). Copy `examples/counter/` as the minimal template; read one
richer example (`todos`, `settings`) before inventing structure.

## Step 1 — project shape (the `just` contract)

```
examples/<name>/
  Cargo.toml        # [lib] + [[bin]] headless smoke + [[example]] "<name>-win"
  src/lib.rs        # pub fn main_app() -> App { App::new(build).stylesheet(include_str!("../app.lss")) }
  src/main.rs       # headless smoke: render a frame, write /tmp/<name>.png
  examples/win.rs   # <name>::main_app().run(Size::new(w, h));   (lumen_shell::RunExt)
  app.lss           # stylesheet (see the styling-lss skill for the working subset)
  tests/            # TestApp integration tests
```

- The `[[example]]` name **must** be `<name>-win` — `just run`/`run-agent`
  look for it. Register the crate in the workspace root `members`.
- Deps: `lumen-core`, `lumen-widgets`, `lumen-layout`, `lumen-render`
  `{ workspace = true }`; dev-dep `lumen-shell`. **In-repo code imports the
  internal crates directly** (ADR-W2) — the `lumen` facade is for scaffolded
  external apps only.
- Recipes: `just run <name>` (window), `just run-hot <name>` (live `.lss`),
  `just run-agent <name>` (window + agent endpoint), `just render <name>`
  (headless), `just test <name>`, `just check` (full gate).

## Step 2 — composition

`Element` is the universal node; widgets are functions/builders producing
one. Compose with the macros and containers:

```rust
use lumen_widgets::{col, row, widgets, BuildCx, Container, Element};

fn build(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i64);
    col![
        widgets::text(format!("Count: {}", count.get(cx.runtime()))).id("readout"),
        row![
            widgets::button("−", move |rt| count.update(rt, |c| *c -= 1)).id("dec"),
            widgets::button("+", move |rt| count.update(rt, |c| *c += 1)).id("inc"),
        ],
    ]
}
```

- Layout is Rust `LayoutStyle` (`Dim::px`/`Dim::pct`, flex fields, `Edges`,
  `Position::Absolute` + `inset`); `Container` for align/padding wrappers.
  **Text nodes ignore explicit `height`** — size a box, put the text in a
  child (see writing-widgets gotchas).
- Typography is Rust too: `e.text_style_mut()` → `font_size`/`weight`/
  `color` (`.lss` typography is parse-only for now).
- Paint on top / hit-test priority = document order: later siblings win.

## Step 3 — the widget catalog (honest availability)

| Module | Constructors |
|---|---|
| `widgets` | `text image row column stack button checkbox slider scroll text_field_basic canvas progress_bar keyed leaf` |
| typed structs | `Button CheckBox Slider TextField TextInput Container Label Accordion PickList ProgressBar Radio Rule Scrollable Space Grid` |
| `widgets_m1` | `spacer divider padding icon switch stepper tabs virtual_list` |
| `widgets_extra` | `radio select tooltip menu grid wrap split_pane text_area modal pane_grid` |
| `widgets_m3` | `bottom_nav navigation_rail app_bar pull_to_refresh date_picker time_picker` |
| `widgets_m4` | `data_grid tree bar_chart rich_text rich_text_editor` |
| `markdown` | `markdown::render` (CommonMark subset) |

**Shipped with W.1:** `Popover::new(cx, name, trigger, content)`
(light-dismiss anchored panel, `.side()`), `Sheet`/`Drawer` (modal panels;
open flag = `{name}.open` signal — set it from any handler), `SearchField`
(editor under `name`, `{name}-input` id inside), and
`Toast`/`Spinner`/`Chip` (in `lumen_widgets::feedback`; Toast is
presentation-only — auto-hide policy is the app's `wake_at`).

**Shipped with W.2:** `Combobox` (filtering; `{name}.selected`),
`ColorPicker` (preset palette → hex in `{name}`), `Skeleton`, `Avatar`,
`Pagination` (`{name}.page`), `RangeSlider` (`{name}.lo`/`.hi`),
`FilePicker` (queues `SystemRequest::OpenFile` via the `Runtime::post`
host mailbox — visible in `app.systemRequests`; native dialog lands with
P.4), `LineChart::element(values, labels)` / `PieChart::element(slices)`,
`AlignBox::center(child)`. Anything unbounded
(lists/tables) must use `virtual_list`/`data_grid` — they're O(visible).

## Step 4 — state rules (the ones that bite)

- State lives in `cx.signal(name, init)`, read at build, mutated **only in
  handlers** via `sig.update(rt, |v| …)` (in-place, pure closure — never
  re-enter the runtime inside it).
- **Handlers capture only `Copy` state** (signal handles, scalars) — an
  owned `String`/`Vec`/clone goes stale (ADR-013). `stable_handler!` makes
  violations fail to compile.
- `build` must be a pure function of signal state — no time/random/IO.
- Memoize expensive subtrees with `cx.scope(name, |cx| …)`; **every signal
  the subtree depends on must be read inside the scope**, or invalidation
  misses it.
- Keyed dynamic lists: `widgets::keyed(...)`; namespace per-item signals
  (`format!("todo-{id}.done")` — dashes only, see Step 6).
- State types stay serializable (default `snapshot` feature): prefer
  sorted `Vec<(K, V)>` over maps with non-string keys. `Box<dyn Trait>`
  is storable via `#[lumen_macros::state_registry]` on the trait +
  `lumen_core::stored_type!(Ty as "tag")` + `register_<trait>::<Ty>("tag")`
  at startup (W.4c) — unregistered tags drop with W0002 on restore.
- Reactive text/background without a rebuild: `text!(cx, "…{sig}…")` and
  `bind!` — background binds are paint-only patches (cheapest update).

## Step 5 — app-level modules (all headless-testable)

| Module | Surface | Note |
|---|---|---|
| `nav` | `Router::{new current navigate navigate_guarded back deep_link can_go_back}` | back stack + guards + deep links; render by matching `router.current()` |
| `forms` | `Validator`, `validate`, `form_field(cx, name, label, validators)` | errors surface as structured data + a11y association |
| `i18n` | `Locale` (`is_rtl`, plurals), `Catalog::{insert with_fallback}` | RTL mirroring is real; test with `input.setLocale` |
| `undo` | `History<T>::{push undo redo can_undo can_redo present}` | pair with a signal holding the present |
| `system` | `MenuModel` (items take `.accel("Ctrl+O")`), `WindowDesc`, `SystemRequest`, runtime clipboard | OS-wired in the shell: clipboard↔arboard, `OpenFile`→rfd dialog (reply lands in the request's `reply` signal), menus→muda (Windows/macOS menubar; on Linux accelerators + `menu.invoke` activate — both run the `cx.register_command` handler under the item's id), `Notification`→desktop notification, `TrayTooltip`→system tray (lazy; its context menu hosts the app `MenuModel`), OS file drops→`Event::Drop`. Agent: `ui.getMenu`/`menu.invoke`/`app.systemRequests` |
| `tasks` | `cx.resource(name, deps, fetch)`, `resource_blocking`, `Spawner` (Inline/Manual/ThreadPool), `Sink` | see Step 7 |
| snapshot | `AppSnapshot`, `Checkpoint` (quiesce/serialize/restore/resume — works on a running instance), `App::run_headless_restored` | whole-app state save/restore |

## Step 6 — stable-id discipline (what makes the app verifiable)

- Every interactive or asserted-on node gets `.id("...")` — unique in the
  window, **`[a-z0-9-]` only**. A dotted id (`#faq.returns`) parses as
  id+class and is unselectable; derived child ids (`{name}-body`) inherit
  the problem — pass dash-cased names.
- Ids are the contract for tests (`app.locator("#save")`), the live agent,
  and a11y. No id ⇒ reachable only by role/text/`:nth` — brittle.
- Duplicate-id detection is not enforced yet (W0001 dead — plan W.4); keep
  them unique by construction.

## Step 7 — data & async (no HTTP client, by design)

ADR-M2: the framework ships the executor seam; **you bring the transport**.

- CPU-ish or blocking work: `cx.resource(name, deps, fetch)` /
  `resource_blocking` — runs on the thread pool, result re-enters through
  the runtime; re-fetches when `deps` change.
- **Never touch the `Runtime` from a worker** — results come back through
  the resource/`Sink` path only.
- **Do not** hand `Runtime::resource(name, future)` a real async future —
  it polls once with a noop waker and never completes (fixed in plan M.5).
- HTTP: bring a client as an **app dependency** (blocking `ureq` on the
  pool is the simple recipe; a tokio-based client runs on *your* runtime
  thread and reports back the same way). WebSocket: see
  `examples/websocket` (tungstenite).
- Tests: `ManualSpawner` makes async deterministic — pump tasks explicitly.

## Step 8 — verify as you go

Follow the `verifying-apps` skill: headless smoke → `TestApp` test with
locator + `expect` → golden if paint changed → one live-window loop for
new interactions. Commit per task (AGENT.md), including the doc-currency
rule if you changed framework behavior.

## References

- `examples/counter` (minimal), `examples/todos` (CRUD+persistence),
  `examples/settings` (multi-screen), `examples/data` (resources),
  `examples/typed_form` (forms), `examples/widget_gallery` (everything).
- `.ai_docs/02-spec-core.md` (amended to the shipped model), 04 §10.
- Skills: `styling-lss`, `verifying-apps`, `writing-widgets` (for new
  widgets), `debugging-lumen`.
