# Widget review — capability audit vs. the code (2026-08-03)

Every widget assessed on three axes: **what a widget of that name is expected to
do** (measured against Flutter / iced / SwiftUI / the Material and WAI-ARIA
patterns), **what the code actually does** (read from source, not from doc
comments), and **the gap**.

Scope: the 58 typed widgets re-exported from `lumen-widgets`. Verified against
`crates/lumen-widgets/src/*.rs` at commit `1adb317`.

> **Headline:** the widget set is *broad and structurally sound* — semantics are
> present nearly everywhere, layout is right, and the recent Flutter-parity pass
> fixed the structural mismatches. The problems are not "missing widgets"; they
> are **four systemic gaps that cut across the whole set**, plus a short list of
> individually broken widgets. The systemic gaps are worth more than any
> per-widget fix, and two of them make the semantic tree *lie to the agent* —
> which strikes at the framework's core value proposition (ADR-009).

---

## 1. Systemic findings (fix these first)

### S1 — Nothing can be disabled. `NodeFlags::DISABLED` is dead code. 🔴 Blocker

Every GUI framework has a disabled state; it is the single most common widget
modifier after the label. In Lumen it does not exist at any layer:

| Layer | State |
|---|---|
| `NodeFlags::DISABLED` (`lumen-core/src/tree.rs:38`) | declared |
| `build_semantics` (`app.rs:4099`) | **reads** the flag → emits `SemState::Disabled` |
| Anything that **sets** the flag | **none — the flag is never written** |
| `Element` | has no `disabled` field |
| Hit-testing / event routing | never consults it |
| `Widget` builders (`Button::disabled()` etc.) | none — 0 of 58 widgets |

Consequences:
- No app can render a disabled control without hand-rolling one.
- `styling-lss` **documents `button:disabled { … }` as a working selector**
  (SKILL.md:88/96). It can never match, because nothing emits the state — a
  documented feature that is unreachable.
- Worse, the naive workaround (pushing `SemState::Disabled` into `states`
  manually) produces a node that *claims* to be disabled but is **still
  clickable**, because hit-testing ignores the flag. That is an active lie to
  the agent and to AT, not just a missing feature.

### S2 — Declared actions with no implementation. 🔴 Blocker

`invoke_action` (`app.rs:2915`) routes exactly three actions: `click`, `focus`,
`dismiss`. But widgets declare more:

| Widget | Declares | Routable? | Keyboard? |
|---|---|---|---|
| `Slider` | `SetValue`, `Increment`, `Decrement` | ✗ none of the three | ✗ no `on_key` |
| `RangeSlider` | same | ✗ | ✗ |
| `Stepper` | `Increment`, `Decrement` | ✗ | ✗ |
| `Menu` items | `Click` | ✗ **no `on_click` at all** | ✗ |

`actions` is the contract the agent and AccessKit read to decide what a node can
do. Declaring `Increment` and then ignoring it means `input.invokeAction` fails
and a screen-reader user has no way to move a slider. This is the same defect
class the P.4 live AT smoke already caught once ("nodes must declare actions") —
inverted: now they declare actions they don't honour.

### S3 — No arrow-key interaction on any non-text widget. 🟠 Serious

App-level keyboard is solid: `Tab`/`Shift+Tab` traversal, `Enter`/`Space`
activation, `Escape` dismiss (`app.rs:1812-1820`), and text editors get vertical
caret nav. But `on_key` is implemented by exactly **three** files
(`text_input`, `text_field`, `widgets_m4`). Every other widget ignores the
keyboard beyond activation.

Missing against the WAI-ARIA authoring patterns every framework implements:
`Slider`/`RangeSlider` (←/→/Home/End), `Radio` group (↑/↓ moves selection),
`Tabs` (←/→ + Home/End), `Select`/`PickList`/`Combobox` (↑/↓ + type-ahead +
Enter), `Menu` (↑/↓, Escape), `Scrollable` (PageUp/Down, Home/End), `Grid`/
`DataGrid`/`Tree` (arrow navigation).

### S4 — Widgets hardcode child ids, so two instances collide. 🟠 Serious

The `writing-widgets` skill requires sub-node ids be namespaced under `name`.
Production widget code violates it:

| Widget | Hardcoded ids | Effect of two instances |
|---|---|---|
| `Stepper` (`widgets_m1.rs:339/342/354`) | `dec`, `inc`, `value` | duplicate ids → `W0001`; selectors ambiguous |
| `DatePicker` (`widgets_m3.rs:499/508`) | `date-prev`, `date-next` | same |
| `TimePicker` (`widgets_m3.rs:835/850/853`) | `hour-{k}`, `min-dec`, `min-inc` | same |
| `PullToRefresh` (`widgets_m3.rs:340/369`) | `refresh-indicator`, `scroll` | same |
| `AppBar` (`widgets_m3.rs:233`) | `title` | same |
| `Modal` (`widgets_extra.rs:500`) | `modal-overlay` | same |

Two steppers on one screen is not an exotic case. `W0001` fires and the first
match wins, so the agent drives the wrong widget — silently.

---

## 2. Individually broken widgets

### `Tooltip` — not a tooltip. 🔴
`widgets_extra.rs:136`. Renders the tip text **permanently visible, stacked in a
column underneath the target**. No hover trigger, no focus trigger, no delay, no
overlay, no positioning. Every framework shows a tooltip on hover/long-press,
floating above content, dismissed on exit. Present behaviour is a caption, and
it changes the layout of whatever it wraps.
*Note:* the pieces to fix it already exist — `Popover` does anchored overlay
positioning with `PopoverSide`, and the element model has `overlay` + hover state.

### `Menu` — inert. 🔴
`widgets_extra.rs:207`. `Menu::new(items: &[&str])` takes **no callback**, and
items carry `focusable: true` + `Action::Click` with **no `on_click`**. Clicking
or activating a menu item does nothing. Also missing: separators, icons,
shortcut/accelerator labels, submenus, disabled items, checkable items.
(Distinct from the *native* menubar `MenuModel` in `system.rs`, which works.)

### `ProgressBar` — no indeterminate mode. 🟠
`progress_bar.rs`. Determinate only (`new(fraction)`). Flutter/Material's
`LinearProgressIndicator` with a null value, iced's indeterminate state, and the
HTML `<progress>` with no `value` all cover "working, unknown duration" — the
most common progress case. `Spinner` is indeterminate but is a different shape.
Also missing: `Role::Progress` range info, buffer/secondary track.

### `Slider` — under-specified and mis-formatted. 🟠
`slider.rs`. Beyond S2/S3: no `step`/`divisions`, no `on_change`/`on_release`
callback, no `.width()`, no vertical orientation, no tick marks or value label.
**Bug:** the accessible value is `format!("{v:.0}")` — a `0.0..1.0` slider
reports `"0"` at every position, so the agent and AT see a constant value.

### `Avatar` — cannot show an image. 🟠
`misc_w2.rs`. Initials + hashed colour only, yet `role: Role::Image`. Every
framework's avatar takes an image with initials as the *fallback*. The `Image`
widget exists, so this is composition, not new capability.

### `Toast` — fire-and-forget only. 🟡
`feedback.rs:74`. No duration/auto-dismiss, no action button ("Undo"), no
`on_dismiss`, no queue/stacking. Material's snackbar contract is
auto-dismiss + one optional action.

### `Chip` — no selection. 🟡
`feedback.rs:198`. Has `on_remove` (delete chip) but no selected/toggle state,
no leading avatar/icon. Material has input/choice/filter/action chips; filter and
choice both need selection.

### `LineChart` / `PieChart` / `BarChart` — no configuration. 🟡
`charts.rs`, `widgets_m4.rs`. Constructed by `element(values, labels)` with no
builder: no axis titles, no legend toggle, no colour override on line/bar, no
grid lines, no tooltip/hover readout, no multi-series. Semantics are present
(good), but these are display-only.

### `TextInput` / `TextField` — missing the standard field options. 🟠
`text_input.rs`, `text_field.rs`. No `placeholder`, no `max_length`, no
`read_only`, no `on_change` (only `on_submit`), no `autofocus`, and **no
password/obscure masking** — the only occurrence of "password" in the crate is a
comment. *Evidence this bites:* the Mercurium wallet had to hand-roll
`lumen_ui/src/widgets/masked_input.rs` precisely because the framework offers
neither masking nor (until this week) caret-follow scrolling.

---

## 3. Widgets that are in good shape (do not touch)

`Grid` (full builder: resizable/zoomable/scrollbars/virtualized/headers +
`&Runtime` accessors), `DataGrid`, `Tree`, `VirtualList`, `Accordion`,
`PickList`, `Combobox`, `Popover`, `Sheet`/`Drawer`, `ColorPicker`, `Container`,
`Label`, `Button`, `CheckBox`, `Radio`, `Switch`, `Tabs`, `Select`,
`SearchField`, `Scrollable`, `RichText`/`RichTextEditor`/`FindReplaceBar`,
`DatePicker`/`TimePicker` (post-2026-08-02 rework), `Space`, `Rule`, `Skeleton`,
`Pagination`, `AlignBox`, `Icon`, `Canvas`, `Image`, `FilePicker`, `Modal`,
`SplitPane`, `PaneGrid`, `Wrap`, `Spinner`.

Caveat: "good shape" = correct and functional for its stated scope. Most still
inherit S1 (no disabled) and S3 (no arrow keys).

## 4. Gaps in the *set* (widgets that don't exist)

Ordered by how often apps need them: **Card** (the single most common Material
container), **Badge** (notification count), **SegmentedControl / ToggleGroup**,
**ListTile** (icon + title + subtitle + trailing row), **Rating**, **Breadcrumb**.
Not blockers — each is composable today — but their absence is felt when
building a real screen.

---

## 5. Implementation plan

Five phases, ordered by leverage. Each phase is independently shippable and
gated by the existing headless suite + `assert_view_coherent`.

### W1 — Disabled state, end to end (fixes S1) — *highest leverage*
1. `Element::disabled: bool` (default false) + `.disabled(bool)` in
   `impl_common!`, so **every** widget gets it at once.
2. `build_node` sets `NodeFlags::DISABLED`; the flag already reaches semantics.
3. **Hit-testing skips disabled subtrees** (`Tree::hit_visit`) and
   `activate_focused`/`invoke_action` refuse them — so the state cannot lie.
4. Focus traversal skips disabled nodes.
5. Paint: a default dimming (or leave to `.lss` `:disabled`) — decide during
   implementation; the selector then works as `styling-lss` already claims.
6. Tests: disabled button ignores click + `invokeAction`; is skipped by Tab;
   reports `SemState::Disabled`; `:disabled` LSS rule applies.
7. Docs: remove the "documented but unreachable" contradiction in `styling-lss`.

### W2 — Honour declared actions (fixes S2)
1. Route `Increment`/`Decrement`/`SetValue` in `invoke_action` via new optional
   `Element` handlers (`on_increment`/`on_decrement`/`on_set_value`).
2. Wire `Slider`, `RangeSlider`, `Stepper` to them; fix `Slider`'s `{:.0}`
   value formatting (format by range/step, not fixed 0 decimals).
3. Give `Menu` an `on_select(Fn(&Runtime, usize))` and per-item `on_click`.
4. **Audit rule + test:** a node declaring an action must have a handler for it.
   Add it to `audit.rs` so this class cannot regress.

### W3 — Keyboard interaction (fixes S3)
Per the WAI-ARIA authoring practices, in dependency order:
`Slider`/`RangeSlider` (←/→/Home/End, ±step) → `Radio` group (↑/↓) → `Tabs`
(←/→/Home/End) → `Select`/`PickList`/`Combobox` (↑/↓/Enter/type-ahead) → `Menu`
(↑/↓/Escape) → `Scrollable` (PageUp/Down/Home/End) → `Grid`/`DataGrid`/`Tree`
(2-D arrows). Each gets a headless key-injection test.

### W4 — Id namespacing (fixes S4)
Mechanical: `Stepper`, `DatePicker`, `TimePicker`, `PullToRefresh`, `AppBar`,
`Modal` derive child ids from `name` (`{name}-inc`, `{name}-date-prev`, …).
Add a regression test that renders **two** of each and asserts no `W0001` and
that both instances are independently drivable. Update any example/test
selectors that relied on the bare ids.

### W5 — Per-widget capability fills
In priority order (each independently shippable):
1. `TextInput`/`TextField`: `placeholder`, `password`, `max_length`,
   `read_only`, `on_change`. Then **delete Mercurium's `masked_input.rs`** and
   re-point it at the framework — that is the acceptance test.
2. `Tooltip`: rebuild on the `Popover` overlay machinery — hover/focus trigger,
   delay, anchored side, no layout impact.
3. `ProgressBar`: indeterminate mode (animated, `cx.animate()` like `Spinner`).
4. `Avatar`: optional image with initials fallback.
5. `Toast`: duration + auto-dismiss, optional action, `on_dismiss`.
6. `Chip`: selected/toggle state + leading icon.
7. Charts: builder options (axis titles, legend, colours, grid lines).
8. New widgets: `Card`, `Badge`, `SegmentedControl`, `ListTile`.

### Ordering rationale
W1 and W2 are blockers because they are **correctness** issues — the semantic
tree currently misrepresents the UI, and that is the one thing this framework
cannot afford to get wrong (ADR-009: "one schema … prevents drift between what
tests query and what the AI sees"). W4 is small and prevents silent
mis-targeting. W3 is the biggest volume of work but is purely additive. W5 is
feature work and can be paced.
