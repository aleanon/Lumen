# 02 — Core Specification (normative)

This document defines the public contracts of `lumen-core`, `lumen-layout`, and `lumen-render` that other crates and user code depend on. Signatures here are binding; internal implementation is free. Where a signature must change to compile, keep semantics identical and record the amendment (rule 1 of 00).

> **Amended 2026-07-09** to the shipped model per the docs↔code audit and
> ADR-W1/ADR-W2 (`docs/plan-remediation-2026-07.md`): the widget model is
> `LeafWidget` + composite functions (§3), `run` lives on `RunExt` (§8),
> and items not yet built are marked *planned* with their plan task.

## 1. Workspace & naming
Workspace crates: `lumen-core`, `lumen-layout`, `lumen-render`, `lumen-text`, `lumen-style`, `lumen-widgets`, `lumen-shell`, `lumen-test`, `lumen-agent`, `lumen-cli`, plus `lumen` (facade re-exporting the public API; user code depends only on `lumen` and `lumen-test`). Examples live in `examples/`, benchmarks in `benches/`, golden images in `tests/golden/<platform>/`.

Geometry types: re-export from `kurbo` (`Point`, `Size`, `Rect`, `Affine`, `Vec2`, `Insets`). Color: `lumen_core::Color` — f32 RGBA, linear-light internally, sRGB at API boundaries; constructors `Color::srgb8(r,g,b,a)`, `Color::from_hex("#rrggbbaa")`.

## 2. Identity

```rust
/// Runtime identity of a live node. Dense, reused after removal (generational).
pub struct NodeIndex { index: u32, generation: u32 }

/// Author-assigned identity, stable across rebuilds, reloads, and sessions.
/// Used by state keys, test locators, and the agent protocol.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct StableId(pub SmolStr);
```

Every node also has an **identity path**: the sequence of (component type, slot index or explicit key) from the root. `StableId`s set via `.id("...")` must be unique within their window; duplicates are a runtime diagnostic (`W0001`), first match wins for selectors.

## 3. Elements, widgets, components

The widget model (**amended per ADR-W1**, replacing the earlier unified
`Widget` trait): **composites are plain functions** returning `Element`
(`fn counter(cx: &mut BuildCx, props: …) -> Element`); **custom leaves**
implement `LeafWidget` and mount via `NodeContent::Custom` /
`widgets::leaf()`:

```rust
/// A description of a widget subtree. Cheap, built every rebuild.
/// Common fields (id/role/style/children/handlers) are flat; mutually-
/// exclusive leaf content lives in `content: NodeContent`
/// (Box/Text/Image/Canvas/Custom).
pub struct Element { /* see lumen-widgets/src/element.rs */ }

pub trait LeafWidget: 'static {
    fn measure(&self, …) -> Size;
    fn paint(&self, …);
    /// MANDATORY — how the agent and a11y see the leaf.
    fn semantics(&self, node: &mut SemanticsNode);
    /// W.0 (shipped 2026-07-10): first refusal on events at this leaf —
    /// pointer events at the hit-test target, key/text at the focused
    /// node. `&self` is deliberate (ADR-013: durable state lives in
    /// signals, written through `rt`); `Handled` consumes the event
    /// (Element-level `on_*` handlers and default routing are skipped).
    fn event(&self, event: &Event, bounds: Rect, rt: &Runtime) -> EventStatus {
        EventStatus::Ignored
    }
}
```

**Builder surface.** Every widget exposes the common modifiers (via
`impl_common!`): `.id(impl Into<StableId>)`, `.class(&str)` (repeatable),
`.background(…)`, `.style(LayoutStyle)`, `.element[_mut]()`. Event handlers
are the typed `Element` fields `on_click` / `on_key` / `on_wheel` /
`on_drag` / `on_text` / `on_drop` / `on_dismiss` / `on_caret_set` (exact
signatures in the `writing-widgets` skill); custom leaves additionally get
the `LeafWidget::event` hook (W.0).

*Scope decisions (W.3, 2026-07-10):* the earlier-specced `.key(impl Hash)`
element modifier is **superseded** by `widgets::keyed()` / `cx.scope(key,
…)` — identity is established *where the subtree is built* (that is what
makes memoization and list-GC possible), so a post-hoc tag on a built
element cannot provide it. A generic `.on(EventKind, handler)` is likewise
**superseded** by the typed `on_*` fields plus the leaf `event()` hook —
a stringly-generic registration adds no capability over the typed surface.
*Still planned:* `.style(Style)` taking the typed 04 §8 style (pairs with
the Phase-B cascade-origin work, B.6b).

**Components** are functions; memoization is signal-based via `cx.scope`
(a scope whose recorded signal reads are current returns its cached
subtree). *Planned:* the `#[component]` attribute macro with
`PartialEq`-on-props memoization (plan W.3).

## 4. Signals & state store (binding discipline)

```rust
pub trait State: serde::Serialize + serde::de::DeserializeOwned + 'static {}
impl<T: Serialize + DeserializeOwned + 'static> State for T {}

pub struct Signal<T: State>(/* opaque copyable handle */);

impl<T: State> Signal<T> {
    pub fn get(&self, cx: &impl ReadCx) -> T where T: Clone;
    pub fn with<R>(&self, cx: &impl ReadCx, f: impl FnOnce(&T) -> R) -> R;
    pub fn set(&self, cx: &impl WriteCx, value: T);
    pub fn update(&self, cx: &impl WriteCx, f: impl FnOnce(&mut T));
}

impl BuildCx {
    /// Creates or re-attaches state. Key = scope prefix + `name`.
    pub fn signal<T: State>(&mut self, name: &str, init: impl FnOnce() -> T) -> Signal<T>;
    /// Signal-read-memoized subtree (the shipped memoization primitive).
    pub fn scope(&mut self, name: &str, f: impl FnOnce(&mut BuildCx) -> Element) -> Element;
    /// Async data keyed by (name, deps): re-fetches when deps change.
    pub fn resource<T: State>(&mut self, name: &str, deps: …, fetch: …) -> Resource<T>;
}
// `cx.memo` / `cx.effect` (W.3): scope-key-prefixed forwards to the
// spec-shaped `Runtime::memo`/`Runtime::effect`.
// NOTE: `Runtime::resource(name, fut)` (the future-taking form) currently
// polls once with a noop waker — do NOT hand it a real async future; use
// the (name, deps, fetch) form / `resource_blocking` on the thread pool.
// Fixed by ADR-M2's executor-seam work (plan M.5).
```

Rules (enforced by the type system where possible, by review otherwise):
- Reading a signal inside `build`/`memo`/`effect` subscribes that scope; writing schedules exactly the subscribed scopes — no whole-tree work.
- Writes are batched per event-loop turn; effects run after rebuild, before paint.
- **The state store is the only retained mutable state.** Widgets are rebuilt-from-scratch descriptions; anything that must survive a rebuild, a hot reload, or a snapshot restart goes in the store.
- **Forbidden in stored state:** closures, function pointers, raw pointers, channels, handles to OS resources. Trait objects only via the `#[state_registry]` typetag-style mechanism: `Box<dyn StoredTrait>` where every impl is registered, serialized by registry name, vtables rebuilt on load. *(Shipped — plan W.4c: `#[lumen_macros::state_registry]` on the trait generates the `Box<dyn Trait>` serde envelope (`{type, value}`) + a `register_<trait>::<T>("tag")` fn; concrete types declare tags with `lumen_core::stored_type!(Ty as "tag")` and register at startup, before any restore. An unregistered tag in a snapshot becomes a `W0002` drop.)*
- Snapshot format: self-describing, field-tagged (postcard is NOT acceptable; use `serde_json` in dev, optionally CBOR later). Missing new fields take `Default`; unknown old fields are dropped with diagnostic `W0002`. *(The `State: Serialize + DeserializeOwned` bound is gated on the default-on `snapshot` feature; `--no-default-features` relaxes it to `'static` and drops serde_json — the deliberate lean-build deviation.)*

```rust
/// Checkpoint protocol — required for hot reload tiers 2–3 and future linker project.
/// Snapshot builds only. Implemented by `Headless` (W.4b).
pub trait Checkpoint {
    fn quiesce(&mut self);                     // park at a safe point (graph at fixpoint)
    fn serialize_state(&self) -> AppSnapshot;  // entire store + host extras (focus)
    fn restore_state(&mut self, snap: AppSnapshot) -> Vec<Diagnostic>;
    fn resume(&mut self);
}
```

*(Status: shipped 2026-07-10 (plan W.4b) with `AppSnapshot` as the snapshot
type — the spec's "store + window/session extras". `restore_state` works on a
**running** instance: existing signals adopt in place
(`Runtime::adopt_pending_live`, scheduling subscribers like a normal write),
signals re-created by the forced rebuild adopt from the staged snapshot, and
leftovers surface as `W0002`. The fresh-boot path remains
`App::run_headless_restored`.)*

## 5. Hot-data SoA (internal layout, observable invariants)

`lumen-core` maintains parallel arrays indexed by `NodeIndex.index`:
`bounds: Vec<Rect>` (window coords, post-layout), `transform: Vec<Affine>`, `opacity: Vec<f32>`, `clip: Vec<Option<Rect>>`, `flags: Vec<NodeFlags>` (bitflags: VISIBLE, DIRTY_LAYOUT, DIRTY_PAINT, FOCUSABLE, HIT_TESTABLE, DISABLED, HOVERED, FOCUSED, PRESSED), `z: Vec<u32>`, `parent: Vec<NodeIndex>`, `first_child/next_sibling: Vec<NodeIndex>` (intrusive tree links).

Binding invariants:
- Hit-testing and culling are implemented as scans/walks over these arrays only — never via widget trait calls.
- Hit-test order: highest z first, then reverse document order; respects `clip` and `HIT_TESTABLE`.
- `bounds` for any node equals the rect reported in semantics and in `ui.getLayout` — one source of truth.

## 6. Events

```rust
pub enum Event {
    PointerDown(PointerEvent), PointerUp(PointerEvent), PointerMove(PointerEvent),
    PointerEnter(PointerEvent), PointerLeave(PointerEvent),
    Wheel(WheelEvent),
    KeyDown(KeyEvent), KeyUp(KeyEvent),
    TextInput(TextInputEvent),          // post-IME committed text
    ImePreedit(ImeEvent),
    FocusIn, FocusOut,
    WindowResized(Size), ThemeChanged(ThemeKind), Timer(TimerToken),
    Gesture(GestureEvent),              // tap, double-tap, long-press, pan, pinch (M3 fleshes out)
    Custom(Box<dyn AnyEvent>),
}
pub struct PointerEvent { pub pos: Point, pub button: PointerButton, pub pointer: PointerKind, pub modifiers: Modifiers, pub click_count: u8 }
```

Dispatch: capture phase root→target, bubble phase target→root; `EventStatus::Handled` stops bubbling. Focus: `Tab`/`Shift+Tab` traversal over FOCUSABLE nodes in document order; arrow-key traversal inside composite controls is the widget's job. Synthesized input (test/agent) enters the same queue as OS input — there is exactly one input path.

## 7. Display list

```rust
pub enum DrawCmd {
    Rect { rect: Rect, brush: Brush, radii: CornerRadii, border: Option<Border> },
    Path { path: BezPath, brush: Brush, style: FillOrStroke },
    Image { id: ImageId, src_rect: Rect, dst_rect: Rect, quality: Filter },
    GlyphRun { run: GlyphRunId, brush: Brush },
    PushLayer { clip: Option<RoundedRect>, opacity: f32, transform: Affine, blend: BlendMode },
    PopLayer,
    Shader { id: ShaderId, rect: Rect, uniforms: UniformBlock },   // CPU backend: deterministic fallback fill
}
pub enum Brush { Solid(Color), LinearGradient(...), RadialGradient(...), ConicGradient(...) }
```

Both backends consume this list. CPU backend is bit-deterministic given identical input (fixed seed, no time-dependent dithering). Damage tracking: each frame computes the union of dirty node bounds; backends may re-render only that region.

## 8. App entry & windowing

```rust
pub struct App;
impl App {
    pub fn new(root: impl Fn(&mut BuildCx) -> Element + 'static) -> Self;
    pub fn stylesheet(self, lss: &str) -> Self;           // also loadable from file via CLI
    pub fn run_headless(self, size: Size) -> Headless;     // CPU renderer, no OS deps
}
// The windowed entry lives in lumen-shell (amended): `lumen_shell::run(app,
// size)` / the `RunExt` extension trait — `app.run(Size::new(w, h))` — which
// returns when the window closes (not `-> !`).
//
// Native menus (P.3c): `cx.set_menu(MenuModel)` declares the menu from build
// (change-detected; ids double as `cx.register_command` names — activation
// runs the bound command). Items may carry `accel("Ctrl+O")` chords. The
// shell realizes the model via muda on Windows (hwnd) / macOS (nsapp);
// Linux/winit has no menubar attachment point (muda is GTK-bound), so there
// accelerators — matched by the shell — and the agent's `menu.invoke`
// activate items, and the system tray's context menu hosts the same
// `MenuModel` natively (P.3e).
//
// Multi-window (P.3d): `App::window(WindowDesc, |cx| …)` declares a
// secondary window with its own root closure; `Headless::open_window(id)`
// realizes it as an independent render pipeline (own tree/layout/paint/
// focus at the declared size) over the SAME shared `Runtime` — cross-window
// reactivity is shared signals (a write in one window re-renders any window
// that reads it on its next pump). The shell (P.3d-2) realizes every
// declaration as a real OS window — loop keyed by winit WindowId, its own
// renderer/surface/scale, pointer/keys/wheel/resize/drop routed per window;
// any injected input schedules a redraw of every window (untouched windows
// pump as dirty-checked no-ops). Menus/accelerators, IME composition,
// clipboard bridging, the AT adapter, and the agent endpoint stay bound to
// the MAIN window (per-window agent verbs are future work).
//
// OS services (P.3e): `SystemRequest::Notification` → desktop notification;
// `SystemRequest::TrayTooltip` → lazy system tray (created on first request;
// tooltip + title text; menu = the app `MenuModel`). OS file drops arrive as
// the same `Event::Drop` headless tests synthesize.
pub struct Headless;  // used by lumen-test and lumen-agent in headless mode
impl Headless {
    pub fn pump(&mut self) -> FrameStats;                  // process queue, layout, paint
    pub fn inject(&mut self, ev: Event);
    pub fn screenshot(&mut self) -> RgbaImage;
    pub fn semantics_json(&self) -> serde_json::Value;     // schema in 03
}
```

## 9. Diagnostics
All warnings/errors are `Diagnostic { code: &'static str, severity, message, span: Option<SourceSpan>, node: Option<StableId> }`, serialized to JSON on the agent protocol and printed human-readably on stderr. Codes are stable API: `E####` errors, `W####` warnings; maintain a registry table in `lumen-core/diagnostics.md`. Initial assignments: W0001 duplicate StableId, W0002 dropped state field, E0101 .lss parse error, E0102 unknown style property (with did-you-mean), W0103 layout overflow, E0201 shader compile error, W0301 missing semantics on focusable leaf.

*Status:* every registered code is emitted — W0002, E0101, E0102, E0103
(type mismatches, B.7a), E0104, W0103/W0104/W0105 + **W0001**
(duplicate StableId, W.4a) + **W0301** (unnamed focusable leaf, W.4a) via
the audit lint, E0201, W0401 (i18n missing key), E0701 (contained panic).
The defined-but-dead bucket from the 2026-07 audit is empty.

## 10. Built-in widget set
**M0 primitives (10):** Text, Image, Row, Column, Stack, Scroll, Button, TextFieldBasic (single-style, pre-IME), Checkbox, Slider.
**M1 additions (to 30 total):** RichText, Icon, Spacer, Divider, Grid, Wrap, Padding, Align, SplitPane, TextField (full IME), TextArea, Radio, Switch, Stepper, Select, Tooltip, Popover, Menu, Tabs, VirtualList.
**M2:** Dialog, Sheet, Drawer, Toast, ProgressBar, Spinner, Badge, Chip, Accordion, SearchField.
**M3 (mobile):** BottomNav, NavigationRail, AppBar, pull-to-refresh on Scroll, DatePicker, TimePicker.
**M4:** VirtualTable/DataGrid, Tree, Combobox, ColorPicker, charts (line/bar/pie), Skeleton, Avatar, Pagination, RangeSlider, FilePicker, RichTextEditor.

Every widget ships with: rustdoc + example, `.lss`-styleable parts documented (type name + parts as classes, e.g. `slider .track`, `slider .thumb`), semantics (role/states/actions per 03 §2), keyboard map, golden test, semantic-tree test.

*Status (2026-07-10, W.1+W.2 ✅):* **M0 10/10, M1 19/20, M2 10/10, M3
6/6, M4 10/11.** W.1: `Popover` (light-dismiss anchored panel,
`.side(Above|Below)`; screen-edge auto-flip deferred — needs post-layout
placement), `Sheet`/`Drawer` (signal-keyed `{name}.open`, full-window
scrim, panel anchored bottom/left/right), `SearchField`, and
`Toast`/`Spinner`/`Chip` promoted into `lumen_widgets::feedback`. W.2:
`Combobox` (filtering dropdown; selection in `{name}.selected`),
`ColorPicker` (preset palette; hex in `{name}`; native dialog waits for
P.4), `Skeleton`, `Avatar`, `Pagination` (`{name}.page`), `RangeSlider`
(`{name}.lo`/`.hi`, nearer-thumb drag), `FilePicker` (rides the new
`Runtime::post` host mailbox → `SystemRequest::OpenFile`; path lands in
`{name}.path` when the shell fulfils it, P.4), `LineChart`/`PieChart`
leaves (`charts::*::element(…)`), and the standalone `AlignBox`.
Remaining: `RichTextEditor` polish is M4's last row (a basic
`rich_text_editor` exists in `widgets_m4`); the M1 draft's `.key()`/
generic `.on()` were superseded (W.3 scope decisions). Widget
parts-as-classes shipped for slider/progress/range-slider (B.7).

## 11. Versioning
Pre-1.0: workspace-wide lockstep version `0.x`. Public-contract changes require a decision-log entry. The facade crate `lumen` re-exports everything user-facing. **Facade rule (amended per ADR-W2):** *in-repo examples* may depend on the internal crates directly (they double as framework tests); **scaffolded user apps (`lumen new`) are facade-only** — user code depends on `lumen` and `lumen-test` alone.
