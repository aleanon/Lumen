# 02 — Core Specification (normative)

This document defines the public contracts of `lumen-core`, `lumen-layout`, and `lumen-render` that other crates and user code depend on. Signatures here are binding; internal implementation is free. Where a signature must change to compile, keep semantics identical and record the amendment (rule 1 of 00).

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

```rust
/// A description of a widget: type + props + children. Cheap, built every rebuild.
pub struct Element { /* opaque */ }

pub trait Widget: 'static {
    /// Composite widgets implement this and nothing else.
    fn build(&mut self, cx: &mut BuildCx) -> Element { Element::leaf() }

    // Leaf widgets implement the following. Defaults are correct for composites.
    fn layout(&mut self, cx: &mut LayoutCx, bc: &BoxConstraints) -> Size { cx.layout_children_default(bc) }
    fn paint(&mut self, cx: &mut PaintCx, scene: &mut SceneBuilder) {}
    fn event(&mut self, cx: &mut EventCx, event: &Event) -> EventStatus { EventStatus::Ignored }

    /// MANDATORY for leaf widgets; composites inherit semantics from children.
    fn semantics(&self, node: &mut SemanticsNode) {}

    fn type_name(&self) -> &'static str { core::any::type_name::<Self>() }
}
```

```rust
pub struct BoxConstraints { pub min: Size, pub max: Size }
pub enum EventStatus { Handled, Ignored }
```

**Builder surface.** Every widget constructor returns an `ElementBuilder` with the universal modifiers:

```rust
impl ElementBuilder {
    pub fn id(self, id: impl Into<StableId>) -> Self;
    pub fn class(self, class: &str) -> Self;            // repeatable
    pub fn style(self, style: Style) -> Self;            // typed inline style (see 04 §8)
    pub fn key(self, key: impl Hash) -> Self;            // identity in dynamic lists
    pub fn on(self, event: EventKind, handler: impl Handler) -> Self;
}
```

**Components** are functions: `fn counter(cx: &mut BuildCx, props: CounterProps) -> Element`. The `#[component]` attribute macro wraps a function into a `Widget` with memoized props (`PartialEq` on props skips rebuild). Children compose via `Row((a, b, c))`-style tuples and `for`-loops with `.key(...)`.

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
    /// Creates or re-attaches state. Key = identity path + `name`.
    pub fn signal<T: State>(&mut self, name: &str, init: impl FnOnce() -> T) -> Signal<T>;
    pub fn memo<T: PartialEq + State>(&mut self, name: &str, f: impl Fn(&ReadScope) -> T + 'static) -> Memo<T>;
    pub fn effect(&mut self, name: &str, f: impl Fn(&ReadScope) + 'static);
    pub fn resource<T: State>(&mut self, name: &str, fut: impl Future<Output = T> + 'static) -> Resource<T>;
}
```

Rules (enforced by the type system where possible, by review otherwise):
- Reading a signal inside `build`/`memo`/`effect` subscribes that scope; writing schedules exactly the subscribed scopes — no whole-tree work.
- Writes are batched per event-loop turn; effects run after rebuild, before paint.
- **The state store is the only retained mutable state.** Widgets are rebuilt-from-scratch descriptions; anything that must survive a rebuild, a hot reload, or a snapshot restart goes in the store.
- **Forbidden in stored state:** closures, function pointers, raw pointers, channels, handles to OS resources. Trait objects only via the `#[state_registry]` typetag-style mechanism: `Box<dyn StoredTrait>` where every impl is registered, serialized by registry name, vtables rebuilt on load.
- Snapshot format: self-describing, field-tagged (postcard is NOT acceptable; use `serde_json` in dev, optionally CBOR later). Missing new fields take `Default`; unknown old fields are dropped with diagnostic `W0002`.

```rust
/// Checkpoint protocol — required for hot reload tiers 2–3 and future linker project.
pub trait Checkpoint {
    fn quiesce(&mut self);                       // park event loop at a safe point
    fn serialize_state(&self) -> StateSnapshot;  // entire store + window/session extras
    fn restore_state(&mut self, snap: StateSnapshot) -> Vec<Diagnostic>;
    fn resume(&mut self);
}
```

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
    pub fn run(self) -> ! ;                                // winit shell
    pub fn run_headless(self, size: Size) -> Headless;     // CPU renderer, no OS deps
}
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

## 10. Built-in widget set
**M0 primitives (10):** Text, Image, Row, Column, Stack, Scroll, Button, TextFieldBasic (single-style, pre-IME), Checkbox, Slider.
**M1 additions (to 30 total):** RichText, Icon, Spacer, Divider, Grid, Wrap, Padding, Align, SplitPane, TextField (full IME), TextArea, Radio, Switch, Stepper, Select, Tooltip, Popover, Menu, Tabs, VirtualList.
**M2:** Dialog, Sheet, Drawer, Toast, ProgressBar, Spinner, Badge, Chip, Accordion, SearchField.
**M3 (mobile):** BottomNav, NavigationRail, AppBar, pull-to-refresh on Scroll, DatePicker, TimePicker.
**M4:** VirtualTable/DataGrid, Tree, Combobox, ColorPicker, charts (line/bar/pie), Skeleton, Avatar, Pagination, RangeSlider, FilePicker, RichTextEditor.

Every widget ships with: rustdoc + example, `.lss`-styleable parts documented (type name + parts as classes, e.g. `slider .track`, `slider .thumb`), semantics (role/states/actions per 03 §2), keyboard map, golden test, semantic-tree test.

## 11. Versioning
Pre-1.0: workspace-wide lockstep version `0.x`. Public-contract changes require a decision-log entry. The facade crate `lumen` re-exports everything user-facing; nothing in user examples may import `lumen_core` directly.
