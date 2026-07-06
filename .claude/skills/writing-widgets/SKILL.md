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
  role/label is invisible to both. (`.ai_docs/03-spec-semantics-agent.md`.)
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
`Prop<T>`/`Dynamic<T>` (F3); for keyed lists / conditional structure use
`cx.scope` / `For`, not a binding.

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
not just that a signal moved — a layout can be wrong while the state is right.

## Before you commit

- `cargo fmt --all`
- `cargo clippy -p lumen-widgets --all-targets` — no warnings (docs included)
- `cargo test -p lumen-widgets <name>` green, incl. `assert_view_coherent`
- `cargo build --workspace` if you changed a shared handler signature or export
- Commit with a clear message describing what the widget does (`AGENT.md`).

## References

- `.ai_docs/02-spec-core.md` — element/build model, `BuildCx`, signals.
- `.ai_docs/03-spec-semantics-agent.md` — roles, actions, states.
- `.ai_docs/07-decision-log.md` — ADR-013 (handler currency), F1–F4 reactivity.
- `.ai_docs/05-spec-testing.md` — headless/coherence testing.
- Widgets: `button.rs`, `check_box.rs`, `slider.rs`, `text_input.rs`, `grid.rs`.
