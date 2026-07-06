---
name: writing-widgets
description: Use when creating or modifying a widget in the Lumen framework (crates/lumen-widgets/src/*.rs, or a reusable Element-producing component in an example). Encodes the canonical widget shape, state/handler rules (ADR-013), semantics for the agent, the hard-won layout gotchas, registration, and the headless test pattern — so widgets come out consistent and correct.
---

# Writing a Lumen widget

A widget is a typed builder that produces one `Element` (a subtree). It lowers to
`Element` via `From`/`.into()` (or `.build(cx)`), composes with `col!`/`row!`/
`Container`, and — if stateful — keeps its state in signals keyed by a `name`.

Work in small steps and **commit per widget/task** (see `AGENT.md`). Read a
neighbouring widget first (`button.rs` stateless, `check_box.rs`/`slider.rs`
stateful, `grid.rs` builder) — match its shape, doc density, and idioms.

## Step 1 — pick the shape

| Shape | When | `new` signature | Lowering |
|---|---|---|---|
| **Stateless** | no owned state (Button, Label, Space) | `new(args) -> Self` | `impl_common!` → `.into()` |
| **Self-stateful** | owns 1 piece of state (CheckBox, Slider, TextInput) | `new(cx: &BuildCx, name: &str, …) -> Self` | `impl_common!` → `.into()` |
| **Builder** | many options and/or per-item callbacks (Grid) | `new(…) -> Self`, chained setters, `build(cx) -> Element` | explicit `.build(cx)` |

State lives in `cx.signal(name, init)`, **not** in the struct. Read the current
value to render (`sig.get(cx.runtime())`); mutate only inside handlers.

## Step 2 — copy the template

### Stateless / self-stateful (the common case)

```rust
//! [`Toggle`] — one-line summary. Its `Element` is built inside [`Toggle::new`];
//! the state lives in a signal keyed by `name`.               // (self-stateful)

use crate::widget::impl_common;
use crate::{BuildCx, Element};
use lumen_core::semantics::{Action, Role, State as SemState};
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use std::rc::Rc;

/// A … . Click (or Space when focused) toggles the boolean stored under `name`.
pub struct Toggle {
    el: Element,
}

impl Toggle {
    /// A toggle labelled `label`, state stored under `name`.
    pub fn new(cx: &BuildCx, name: &str, label: impl Into<String>) -> Toggle {
        let label = label.into();
        let on = cx.signal(name, || false);          // state handle (Copy)
        let is = on.get(cx.runtime());               // read to render this build

        let el = Element {
            role: Role::Checkbox,                    // semantics: the agent + a11y
            label: label.clone(),
            focusable: true,
            actions: vec![Action::Click, Action::Focus],
            states: vec![if is { SemState::Checked } else { SemState::Unchecked }],
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: Some(Align::Center),
                column_gap: Dim::px(8.0),
                ..LayoutStyle::default()
            },
            // Handler: capture only Copy state (the signal handle). See ADR-013.
            on_click: Some(Rc::new(move |rt| on.update(rt, |b| *b = !*b))),
            children: vec![/* … */ Element::text(label)],
            ..Element::default()
        };
        Toggle { el }
    }

    /// A chained modifier: mutate `self.el`, return `self`.
    pub fn color(mut self, c: lumen_core::Color) -> Toggle {
        if let Some(ts) = self.el.text_style_mut() { ts.color = c; }
        self
    }
}

impl_common!(Toggle);   // adds .id/.class/.background/.style/.element[_mut] + From<Toggle> for Element
```

### Builder (many options + callbacks)

Config setters take `mut self`, return `Self`; store callbacks as
`Rc<dyn Fn(...)>`; a final `build(self, cx: &BuildCx) -> Element` does the work.
Namespace all sub-state under `name` (`{name}.sx`, `{name}.text`, …) so multiple
instances don't collide. See `grid.rs` end-to-end. Expose read accessors as
statics when the app needs them (`Grid::zoom_of(cx, name)`).

## Step 3 — the rules (non-negotiable)

- **Semantics are mandatory, not optional.** Set `role`, `label`, and the
  relevant `actions`/`states`/`value`/`focusable`. This is how the agent sees and
  drives the UI and how a11y works — the framework's core value. A node with no
  role/label is invisible to both. The vocabulary is closed enums in
  `lumen_core::semantics` (`.ai_docs/03-spec-semantics-agent.md`) — pick from what
  exists; if nothing fits, use `Role::Group` (a container) or `Role::Generic`
  (a leaf), never invent a variant:
  - **`Role`:** `Window Button Checkbox Radio Switch Slider TextInput Text Image
    Link List ListItem Table Row Cell ColumnHeader TabList Tab TabPanel Menu
    MenuItem Dialog Alert Tooltip Progress Group ScrollArea Tree TreeItem ComboBox
    Generic`
  - **`State`:** `Focused Hovered Pressed Disabled Checked Unchecked Mixed Selected
    Expanded Collapsed Readonly Required Invalid Busy`
  - **`Action`:** `Click Focus Blur SetValue Increment Decrement ScrollIntoView
    Expand Collapse Dismiss`
  - A toggle/disclosure mirrors its state both ways: put the boolean pair
    (`Checked/Unchecked`, `Expanded/Collapsed`) in `states` **and** the matching
    `actions` (`Click`, or `Expand`/`Collapse`) so the agent can both read and act.
  - `focusable: true` + `on_click` gives Space/Enter activation for free (the
    framework routes focused-key activation to `on_click`) — you don't wire keys.
- **Handlers capture only stable `Copy` state** — signal/memo handles, scalars —
  **never** an owned snapshot (`String`/`Vec`/`Rc`/a cloned value), which goes
  stale when the handler is retained (ADR-013). When in doubt wrap with
  `stable_handler!(move |rt| …)`, which fails to compile if the closure isn't
  `Copy`. Mutate state inside the handler (`sig.update`/`sig.set`), never read-
  modify-write across the build boundary.
- **`build` must be a pure function of signal state.** No `Date`/random/IO in the
  builder; same state ⇒ same `Element`. The coherence oracle
  (`assert_view_coherent`) asserts `incremental == rebuild_fresh` and will catch
  drift. A memoized subtree (`cx.scope`) **must read every signal its output
  depends on** — deriving a value outside the scope and only using it inside
  makes the cache miss the dependency (a real bug class).
- **Doc every `pub` item.** The crate is `#![warn(missing_docs)]` — struct, every
  pub field, every method, the module (`//!`).
- **State types stay serializable** (the `snapshot` feature). Prefer a sorted
  `Vec<(K, V)>` over `HashMap`/`BTreeMap` with tuple keys (JSON needs string map
  keys). Keep an external-reader **mirror** if handy (TextInput publishes
  `{name}.text` alongside its editor state).

## Handler signatures (exact)

| Field | Type | Params |
|---|---|---|
| `on_click` | `Fn(&Runtime)` | — |
| `on_drag` | `Fn(&Runtime, f64, f64, kurbo::Point)` | `frac_x`, `frac_y` (0..1 of the node), `pos` (window px) |
| `on_wheel` | `Fn(&Runtime, f64, f64, Modifiers)` | `dx`, `dy`, `mods` |
| `on_key` | `Fn(&Runtime, &KeyEvent)` | key event |
| `on_text` | `Fn(&Runtime, &str)` | committed text |
| `on_caret_set` | `Fn(&Runtime, usize, bool)` | byte offset, `extend` |

Sliders/scrollbars read the drag fraction; pixel drags (resize, pan) read `pos`.
For reactive props without a rebuild, prefer the `text!` / `bind!` macros and
`Prop<T>`/`Dynamic<T>` (F3).

**Conditional structure inside a widget = plain conditional `children`.** Show a
subtree only when open? Push it or don't: `children: if is { vec![body] } else {
vec![] }` (see `check_box.rs`'s `tick()`). Do **not** reach for `cx.scope`/`For`
here — those need `&mut BuildCx`, but a self-stateful widget's `new(cx: &BuildCx,
…)` only has `&self`. `cx.scope`/`For` are for `&mut BuildCx` *view functions* and
keyed lists at the app level, not for a widget builder. The build is still a pure
function of the signal, so the coherence oracle covers the toggle.

**Builder that needs the state to shape later setters** (e.g. `.body(children)`
mounting content only when open): snapshot the read value into the struct in
`new` (`Accordion { el, name, is_open }`) so the later setter uses it without
re-touching `cx`. Namespace any tagged sub-nodes under `name` (`{name}-body`).

## Gotchas (hard-won — check every one)

- **A text-bearing element ignores an explicit `height`** (it sizes to the
  glyphs; `width` *is* honoured). To size a text cell, put the label in a **child**
  of a sized box, never set text content on the box you're sizing. (This caused
  the "resize just adds empty space" bug.)
- **Hit-test / paint priority = document order.** Later siblings paint on top and
  win hit-testing. Push interactive overlays (resize handles, thumbs) **after** the
  things they must sit above.
- **`focusable` + `on_click` both fire** on the same press. Keyboard input routes
  to the *focused* node's id, so to make a click start editing, give the click
  target and the editor the **same stable id** — focus lands where keystrokes go.
- **Colours:** `Color::srgb8(r,g,b,a)` is a runtime fn (not `const`); `WHITE` and
  `Color::new_linear(...)` are `const`. Build palettes in a fn, or thread a small
  `Copy` struct — don't reach for `const` `srgb8`.
- **Sizes are logical px** via `Dim::px`/`Dim::pct`; absolute children set
  `Position::Absolute` + `inset`. `Dim::px` takes `f32`.

## Step 4 — register

1. `pub mod <name>;` in `crates/lumen-widgets/src/lib.rs` (alphabetical block).
2. `pub use <name>::<Type>;` in the re-export block below `renderer_override`.
3. If it needs a new runtime dep, add it to the **workspace** `Cargo.toml`
   (ADR-003 whitelist) and the crate — don't pin versions per-crate.

## Step 5 — test headless (deterministic)

Add a `#[cfg(test)] mod tests` in the widget file (see `grid.rs`) that drives it
through the headless runtime and asserts on **state + semantics + bounds**, not
pixels:

```rust
let mut h = App::new(|cx| Toggle::new(cx, "t", "Label").into()).run_headless(Size::new(200.0, 80.0));
h.pump();
h.inject(Event::PointerDown(pe(x, y)));   // click, wheel, drag, text…
h.pump();
let on: Signal<bool> = h.runtime().signal("t", || false);
assert!(on.get(h.runtime()));             // state changed
assert!(h.node_bounds_by_id("id").is_some());  // laid out where expected
h.assert_view_coherent();                 // incremental == rebuild_fresh
```

Assert the *rendered result* where it matters (`node_bounds_by_id`, semantics),
not just that a signal moved — a layout can be wrong while the state is right. For
conditional structure, assert the subtree is **absent** when off (a tagged node's
`node_bounds_by_id` is `None`, or `node_count` is lower) and **present** when on.

Run the module tests with **`--lib`**: `cargo test -p lumen-widgets --lib <name>`.
Without `--lib`, a bare `<name>` filter matches integration-test *files*, runs
**zero** of your unit tests, and still prints `ok` — a false green.

## Step 6 — ship a runnable example and drive it in the live window

A widget isn't done until there's an example that uses it *and you've looked at it
running*. Headless tests prove logic; the live window proves it renders and reacts
in a real shell (GPU, hit-testing, focus) — things headless can't catch.

### 6a. Create an example crate `examples/<name>/`

Mirror `examples/counter/` (the minimal template):

```
examples/<name>/
  Cargo.toml        # [lib] + [[bin]] (headless PNG) + [[example]] "<name>-win" (shell)
  src/lib.rs        # pub fn main_app() -> App { App::new(build).stylesheet(include_str!("../app.lss")) }
  src/main.rs       # headless smoke: main_app().run_headless(size).pump(); write screenshot PNG
  examples/win.rs   # `<name>::main_app().run(Size::new(w, h));`  (use lumen_shell::RunExt)
  app.lss           # minimal stylesheet
```

- `Cargo.toml` deps: `lumen-core`, `lumen-render`, `lumen-widgets`, `lumen-layout`
  (all `{ workspace = true }`); dev-dep `lumen-shell`. The `[[example]]` name **must**
  be `<name>-win` (the `just run` recipes look for it).
- Register the crate in the **workspace root `Cargo.toml`** `members` list.
- In `build`, show the widget doing its thing, and give the interactive trigger a
  **stable `.id(...)`** so the live agent can address it (selectors are CSS-like;
  a unique `#id` is the reliable one — a match must be unambiguous).

### 6b. Headless smoke — look at the frame

```
cargo run -p <name>          # writes /tmp/<name>.png; then Read it to eyeball layout
```
Confirms it builds and lays out before you spin up a window.

### 6c. Drive the live window (the framework's see-and-click ability)

`lumen-shell` embeds the agent protocol into the **running** window: newline-
delimited JSON-RPC over TCP, so you can screenshot and click the real GUI (see
`live-window-agent` in memory / `.ai_docs/03-spec-semantics-agent.md`). Needs a
display (`DISPLAY` set — true on this dev box); if none, skip 6c and rely on 6b +
tests.

1. Launch it in the **background** (release build → first run is slow; wait for
   the port to accept a connection):
   ```
   just run-agent <name>          # window + JSON-RPC on 127.0.0.1:9230
   ```
2. Drive it with a tiny socket client, and **view the screenshots** to verify the
   visible state actually changes across an interaction:
   ```python
   import socket, json, base64
   f = socket.create_connection(("127.0.0.1", 9230)).makefile("rwb")
   def rpc(method, **params):
       f.write((json.dumps({"jsonrpc":"2.0","id":1,"method":method,"params":params})+"\n").encode()); f.flush()
       return json.loads(f.readline())
   def shot(path):  # result.image_base64 is a base64 PNG
       open(path,"wb").write(base64.b64decode(rpc("ui.screenshot")["result"]["image_base64"]))
   shot("/tmp/<name>-before.png")
   rpc("input.click", selector="#<trigger-id>")   # or input.key / input.scroll / input.type
   shot("/tmp/<name>-after.png")
   ```
   Then `Read` both PNGs and confirm the interaction did what it should (e.g. the
   accordion body appeared, the chevron flipped). Other verbs: `ui.getTree`,
   `ui.getLayout {selector}`, `ui.lint` (finds overflow/contrast/clip defects),
   `input.invokeAction {selector, action}` (geometry-free).
3. **Kill the background process** when done (don't leave a window/port open).

## Before you commit

- `cargo fmt --all`
- `cargo clippy -p lumen-widgets --all-targets` — no warnings (docs included)
- `cargo test -p lumen-widgets --lib <name>` green (the `--lib` matters — a bare
  filter can match zero unit tests and still report `ok`), incl. `assert_view_coherent`
- `cargo build --workspace` if you changed a shared handler signature or export
- An `examples/<name>/` crate exists, registered in the workspace `members`; you
  ran it (`cargo run -p <name>`, viewed the PNG) and, where a display is available,
  drove it once in the live window (`just run-agent <name>` → screenshot → click →
  screenshot) and confirmed the visible state changed.
- Commit with a clear message describing what the widget does (`AGENT.md`).

## References

- `.ai_docs/02-spec-core.md` — element/build model, `BuildCx`, signals.
- `.ai_docs/03-spec-semantics-agent.md` — roles, actions, states.
- `.ai_docs/07-decision-log.md` — ADR-013 (handler currency), F1–F4 reactivity.
- `.ai_docs/05-spec-testing.md` — headless/coherence testing.
- Widgets: `button.rs`, `check_box.rs`, `slider.rs`, `text_input.rs`, `grid.rs`.
