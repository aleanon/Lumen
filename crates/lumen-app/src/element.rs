//! Element descriptions and the build context.
//!
//! For M0 an [`Element`] is a concrete description (the full `Widget` trait
//! arrives in T0.10). It carries everything the headless runtime needs to lay
//! out, paint, route events, and emit semantics for one node.

use lumen_core::identity::{fold_id, hash_id, key_name, IdHash, ROOT_ID};
use lumen_core::semantics::{Action, Role, ScrollInfo, State as SemState};
use lumen_core::state::{Runtime, State};
use lumen_core::tasks::{CancelToken, TaskHandle};
use lumen_core::{Color, Dynamic, Signal, StableId};
use lumen_layout::{Dim, Display, FlexDirection, LayoutStyle};
use lumen_render::RgbaImage;
use lumen_text::TextStyle;
use std::cell::Cell;
use std::cell::RefCell;
use std::fmt::Debug;
use std::hash::Hash;
use std::rc::Rc;

/// A click/activate handler. Re-registered every build; never stored (ADR-013).
pub type Handler = Rc<dyn Fn(&Runtime)>;

/// Root-level key holding the stable id currently under the pointer (empty when
/// nothing is). Signal-backed so [`BuildCx::is_hovered`] is a *tracked* read —
/// see that method for why that matters.
pub(crate) const HOVER_SIGNAL: &str = "lumen.hover";

/// A handler that receives a value (W2) — backs [`Action::SetValue`], which the
/// widget parses from the string.
pub type ValueHandler = Rc<dyn Fn(&Runtime, &str)>;
/// A wheel handler receiving the horizontal and vertical delta (logical px) and
/// the modifier state. Most consumers scroll vertically (`dy`); a 2D surface
/// (spreadsheet) uses both, and reads the modifiers (e.g. Ctrl+wheel to zoom).
pub type WheelHandler = Rc<dyn Fn(&Runtime, f64, f64, lumen_core::events::Modifiers)>;
/// A drag handler receiving the pointer's fraction along the node's width and
/// height (`frac_x`, `frac_y`, each clamped to `0.0..=1.0`) **and** the pointer's
/// window-space position. Sliders/scrollbars use the fractions; pixel drags
/// (resizing a column, panning) use the absolute position.
pub type DragHandler = Rc<dyn Fn(&Runtime, f64, f64, kurbo::Point)>;
/// A committed-text handler (text inputs).
pub type TextHandler = Rc<dyn Fn(&Runtime, &str)>;
/// A key handler on the focused node, receiving each `KeyDown` (the node decides
/// what to do — e.g. a list handling PageUp/Down/Home/End/arrows).
pub type KeyHandler = Rc<dyn Fn(&Runtime, &lumen_core::events::KeyEvent)>;
/// A drop handler receiving the dropped payload (T5.2 drag-and-drop).
pub type DropHandler = Rc<dyn Fn(&Runtime, &lumen_core::events::DropData)>;
/// A caret-placement handler for text editors. The app resolves a pointer press
/// or vertical-nav key to a byte offset (via the text engine's geometry) and
/// calls this with `(byte, extend)` — `extend` keeps the selection anchor
/// (drag-select / Shift). Marks an element as an editable text field.
pub type CaretHandler = Rc<dyn Fn(&Runtime, usize, bool)>;
/// An immediate-mode draw callback (E8.1 Canvas): paints into a `Frame` sized to
/// the node's bounds.
pub type CanvasFn = Rc<dyn Fn(&mut lumen_render::canvas::Frame, kurbo::Size)>;

/// A drop shadow cast behind an element's (rounded) box. Approximated by the
/// painter as a stack of translucent rounded rects, so `blur` reads as a soft
/// penumbra without a true gaussian pass.
#[derive(Clone, Copy, Debug)]
pub struct Shadow {
    /// Horizontal offset (px, positive = right).
    pub dx: f64,
    /// Vertical offset (px, positive = down).
    pub dy: f64,
    /// Blur radius (px): how far the penumbra spreads.
    pub blur: f64,
    /// Spread (px): grows the shadow box before blurring.
    pub spread: f64,
    /// Shadow colour (its alpha sets the overall strength).
    pub color: Color,
}

impl Shadow {
    /// A soft, subtle downward shadow (good default for cards).
    pub fn soft() -> Shadow {
        Shadow {
            dx: 0.0,
            dy: 6.0,
            blur: 18.0,
            spread: 0.0,
            color: Color::srgb8(0x0f, 0x17, 0x2a, 0x40),
        }
    }
}

/// A custom leaf widget (E2 — the spec's `Widget` leaf archetype, 02 §3).
/// Third-party / agent-authored leaves implement this to measure, paint, and
/// contribute semantics; they are first-class via [`NodeContent::Custom`] and
/// the runtime treats them like any built-in leaf. `semantics()` is **mandatory**
/// (01 §1.6) — a leaf with no accessible role/label is a bug, not an option.
pub trait LeafWidget {
    /// Intrinsic size in logical px, given the available space.
    fn measure(&self, available: kurbo::Size) -> kurbo::Size;
    /// Paint into `frame` (node-local coords), sized to the node's bounds.
    fn paint(&self, frame: &mut lumen_render::canvas::Frame, size: kurbo::Size);
    /// Accessible (role, name). Drives semantics, test locators, and the agent.
    fn semantics(&self) -> (Role, String);
    /// React to an event delivered to this leaf (W.0, ADR-W1). `bounds` is
    /// the node's window-space rect (interpret pointer positions against
    /// it); state writes go through `rt` — the widget value itself is
    /// rebuilt every frame, so `&self` is deliberate (ADR-013: durable
    /// state lives in signals). Return [`EventStatus::Handled`] to consume
    /// the event: the Element-level `on_*` handlers and default routing are
    /// skipped for it. Default: [`EventStatus::Ignored`].
    fn event(
        &self,
        _event: &lumen_core::events::Event,
        _bounds: kurbo::Rect,
        _rt: &Runtime,
    ) -> lumen_core::events::EventStatus {
        lumen_core::events::EventStatus::Ignored
    }
}

/// A node's leaf content — mutually exclusive by construction (E1): a node is a
/// container, *or* one kind of leaf.
#[derive(Clone, Default)]
pub enum NodeContent {
    /// No leaf content (a box / container).
    #[default]
    None,
    /// A text run and its style.
    Text(String, TextStyle),
    /// A bitmap image.
    Image(RgbaImage),
    /// An immediate-mode canvas draw callback (E8.1).
    Canvas(CanvasFn),
    /// A custom leaf widget (E2): measures/paints/semantics via [`LeafWidget`].
    Custom(Rc<dyn LeafWidget>),
}

/// The accent colour (buttons, highlights, the focus ring).
///
/// SD1: this lives here rather than in `theme` because the runtime's focus ring
/// needs it, and `theme` builds `Element`s from the widget catalogue — so a
/// runtime → theme reference is the one edge that would make the `lumen-app`
/// split cyclic. `theme::accent()` reads this, reversing the direction.
pub fn accent_color() -> Color {
    Color::srgb8(0x1a, 0x73, 0xe8, 0xff)
}

/// A description of one node: type + props + children.
#[derive(Clone)]
pub struct Element {
    /// Author id (`.id("...")`).
    pub id: Option<StableId>,
    /// Accessible role.
    pub role: Role,
    /// Accessible name.
    pub label: String,
    /// Current value (inputs/sliders).
    pub value: Option<String>,
    /// Classes.
    pub classes: Vec<String>,
    /// Supported actions.
    pub actions: Vec<Action>,
    /// Layout style.
    pub style: LayoutStyle,
    /// Background fill.
    pub background: Option<Color>,
    /// Optional border (uniform color + width), drawn on the box edge. A `.lss`
    /// `border` overrides this; for a focused editor the focus ring takes over.
    pub border: Option<lumen_render::Border>,
    /// Corner radius (px).
    pub corner_radius: f64,
    /// Leaf content — text, image, or canvas, mutually exclusive (E1).
    pub content: NodeContent,
    /// Whether the node is keyboard-focusable.
    pub focusable: bool,
    /// Disabled: the node (and its whole subtree) ignores input.
    ///
    /// This is enforced, not advisory — `build_node` clears `HIT_TESTABLE` and
    /// `FOCUSABLE` on the subtree, so a disabled control cannot be clicked,
    /// hovered, dragged, tabbed to, activated, or driven by the agent's
    /// `input.invokeAction`. `SemState::Disabled` reaches semantics and the
    /// `:disabled` `.lss` selector, so what the agent sees matches what the
    /// user can do.
    pub disabled: bool,
    /// Claim focus after a rebuild when nothing is focused yet (e.g. the
    /// primary input of a screen). First autofocus node in document order
    /// wins; it never steals focus the user has placed elsewhere.
    pub autofocus: bool,
    /// Whether the node is elided from semantics (pure layout).
    pub elide_semantics: bool,
    /// Explicit semantic states (e.g. checked/disabled).
    pub states: Vec<SemState>,
    /// Scroll info for scroll containers (semantics).
    pub scroll: Option<ScrollInfo>,
    /// Click handler.
    pub on_click: Option<Handler>,
    /// Wheel handler (scroll containers).
    pub on_wheel: Option<WheelHandler>,
    /// Drag handler (sliders); receives the fraction along the main axis.
    pub on_drag: Option<DragHandler>,
    /// Drag-and-drop drop handler.
    pub on_drop: Option<DropHandler>,
    /// Committed-text handler (text inputs).
    pub on_text: Option<TextHandler>,
    /// Key handler invoked on the focused node for each `KeyDown`.
    pub on_key: Option<KeyHandler>,
    /// Caret-placement handler (editable text fields). Its presence marks the
    /// element as a text editor: the app resolves pointer presses / drags and
    /// vertical-nav keys to a byte offset and calls this.
    pub on_caret_set: Option<CaretHandler>,
    /// The caret byte offset to draw when this field is focused (text editors).
    pub caret_byte: Option<usize>,
    /// The selected byte range `(start, end)` to highlight when focused.
    pub selection: Option<(usize, usize)>,
    /// Light-dismiss handler: fired when a pointer press lands *outside* this
    /// element's bounds, or on Escape. Used for click-away on transient overlays
    /// (dropdowns, popovers, menus, tooltips).
    pub on_dismiss: Option<Handler>,
    /// Adjust the value one step up / down (W2).
    ///
    /// A widget that declares [`Action::Increment`]/[`Action::Decrement`] must
    /// set these — that pair is the contract the agent (`input.invokeAction`)
    /// and AccessKit read, and it is also what arrow-key handling calls, so a
    /// slider is drivable without pixel geometry.
    pub on_increment: Option<Handler>,
    /// Counterpart of [`Element::on_increment`].
    pub on_decrement: Option<Handler>,
    /// Set the value directly from a string (W2) — backs [`Action::SetValue`].
    /// The widget parses it; a value it can't parse is ignored.
    pub on_set_value: Option<ValueHandler>,
    /// Clip descendants to this element's (rounded) bounds — `overflow: hidden`.
    /// Used by scroll viewports so off-screen content doesn't paint outside.
    pub clip: bool,
    /// Paint this element's subtree in a final top pass, above the rest of the UI
    /// and escaping ancestor clips — a portal/overlay (dropdown menus, popovers,
    /// tooltips). Layout/hit-testing are unchanged; only paint order moves.
    pub overlay: bool,
    /// `@media container(…)` reference (B.2b, 04 §6): descendants' container
    /// queries test this element's laid-out size. Set with
    /// [`container`](Self::container).
    pub container: bool,
    /// Optional drop shadow behind the box.
    pub shadow: Option<Shadow>,
    /// Typed inline `.lss` mirror (B.6b, 04 §8): the `Origin::Inline` tier —
    /// beats stylesheet declarations unless they are `!important`. Set with
    /// [`css`](Self::css). Boxed: most elements carry none.
    pub css_inline: Option<Box<lumen_style::Style>>,
    /// If this element is the root a [`BuildCx::scope`] returned, the stable keys
    /// of the signals that scope depends on — projected into semantics (F2) so
    /// the agent can see the reactive structure. Set by `scope`; not authored.
    pub scope_deps: Option<Vec<String>>,
    /// The full scope key when this element is a [`BuildCx::scope`] root
    /// (A.3.1, docs/plan-retained-pipeline.md): lets `build_node` record the
    /// scope's node span — the anchor the retained-graph splice will replace.
    /// Set by `scope`; not authored. `Copy` identity (ADR-021) — a scope root
    /// no longer clones a key string per build.
    pub scope_key: Option<IdHash>,
    /// A memo-hit stub (A.3.2): the real subtree lives behind this `Rc` (the
    /// scope cache's copy). `build_node` copies the scope's retained per-node
    /// work forward when sound, else materializes an owned clone and lowers
    /// it normally. Never authored; set by `BuildCx::scope` on a cache hit.
    #[doc(hidden)]
    pub shared: Option<std::rc::Rc<Element>>,
    /// A reactive binding for this node's text content (F3, option B). When set,
    /// the build evaluates it to produce the string; the `text!` macro emits it.
    ///
    /// F3.5: a change patches in place — no rebuild, no relayout — whenever the
    /// new string measures the same size as the old one. When it does not, the
    /// runtime falls back to a full rebuild, so the box is always right.
    pub dyn_text: Option<Dynamic<String>>,
    /// A reactive binding for this node's background colour (F3). Paint-only, so
    /// a change patches without relayout.
    pub dyn_bg: Option<Dynamic<Color>>,
    /// A reactive binding appending classes (F5.2). Classes drive the `.lss`
    /// cascade (which may change size), so a change is structural (rebuild).
    pub dyn_classes: Option<Dynamic<Vec<String>>>,
    /// Children.
    pub children: Vec<Element>,
}

impl Default for Element {
    fn default() -> Self {
        Element {
            id: None,
            role: Role::Generic,
            label: String::new(),
            value: None,
            classes: Vec::new(),
            actions: Vec::new(),
            style: LayoutStyle::default(),
            background: None,
            border: None,
            corner_radius: 0.0,
            content: NodeContent::None,
            focusable: false,
            disabled: false,
            autofocus: false,
            elide_semantics: false,
            states: Vec::new(),
            scroll: None,
            on_click: None,
            on_wheel: None,
            on_drag: None,
            on_drop: None,
            on_text: None,
            on_key: None,
            on_caret_set: None,
            caret_byte: None,
            selection: None,
            on_dismiss: None,
            on_increment: None,
            on_decrement: None,
            on_set_value: None,
            clip: false,
            overlay: false,
            container: false,
            shadow: None,
            css_inline: None,
            scope_deps: None,
            scope_key: None,
            shared: None,
            dyn_text: None,
            dyn_bg: None,
            dyn_classes: None,
            children: Vec::new(),
        }
    }
}

impl Element {
    /// A flex-row container (pure layout, elided from semantics).
    pub fn row(children: impl Into<Vec<Element>>) -> Element {
        Element {
            role: Role::Group,
            elide_semantics: true,
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                ..LayoutStyle::default()
            },
            children: children.into(),
            ..Element::default()
        }
    }

    /// A flex-column container (pure layout, elided from semantics).
    pub fn column(children: impl Into<Vec<Element>>) -> Element {
        Element {
            role: Role::Group,
            elide_semantics: true,
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..LayoutStyle::default()
            },
            children: children.into(),
            ..Element::default()
        }
    }

    /// Static text.
    pub fn text(s: impl Into<String>) -> Element {
        let s = s.into();
        Element {
            role: Role::Text,
            label: s.clone(),
            content: crate::NodeContent::Text(s, TextStyle::default()),
            ..Element::default()
        }
    }

    /// A push button with a text label.
    pub fn button(label: impl Into<String>) -> Element {
        let label = label.into();
        Element {
            role: Role::Button,
            label: label.clone(),
            actions: vec![Action::Click, Action::Focus],
            focusable: true,
            background: Some(Color::srgb8(0x1a, 0x73, 0xe8, 0xff)),
            corner_radius: 6.0,
            style: LayoutStyle {
                padding: lumen_layout::Edges::all(Dim::px(8.0)),
                ..LayoutStyle::default()
            },
            content: crate::NodeContent::Text(
                label,
                TextStyle {
                    font_size: 16.0,
                    weight: 400.0,
                    color: Color::WHITE,
                    line_height: None,
                    letter_spacing: 0.0,
                    family: None,
                    features: None,
                    variations: None,
                    italic: false,
                    align: lumen_text::TextAlign::Start,
                },
            ),
            ..Element::default()
        }
    }

    /// Mutable access to this node's text style, if it is a text node — lets
    /// helpers (theme typography) restyle a freshly-built text element (E1).
    pub fn text_style_mut(&mut self) -> Option<&mut TextStyle> {
        match &mut self.content {
            NodeContent::Text(_, ts) => Some(ts),
            _ => None,
        }
    }

    /// Set the author id.
    pub fn id(mut self, id: impl Into<StableId>) -> Self {
        self.id = Some(id.into());
        self
    }
    /// Add a class.
    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.classes.push(c.into());
        self
    }
    /// Apply a typed inline style (B.6b) — the `.lss` mirror at
    /// `Origin::Inline`: wins over stylesheet rules unless they are
    /// `!important` (04 §2). The shipped form of the spec's `.style(Style)`
    /// (that name was already taken by `LayoutStyle`).
    pub fn css(mut self, s: lumen_style::Style) -> Self {
        self.css_inline = Some(Box::new(s));
        self
    }
    /// Mark this element as the reference for descendants' `@media
    /// container(…)` queries (04 §6). The size tested is this element's
    /// laid-out size — measured after layout, with one bounded re-pass per
    /// rebuild, so a size change is visible to queries within the same pump
    /// (a further mid-pass change waits for the next one).
    pub fn container(mut self) -> Self {
        self.container = true;
        self
    }
    /// Expose this element as a named widget part (04 §5) — `slider .thumb`
    /// style hooks. Parts are classes; scoping comes from the ancestor chain
    /// (`slider .thumb` only matches inside a slider since B.1), so this is
    /// `class()` with documented intent. Shipped form of the spec's
    /// `cx.part("thumb")`.
    pub fn part(self, name: impl Into<String>) -> Self {
        self.class(name)
    }
    /// Set the background fill.
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
    /// Bind this node's text content to a reactive closure (F3, option B). The
    /// build re-evaluates it each frame the binding's deps change; prefer the
    /// `text!` macro sugar. Only meaningful on a text element.
    ///
    /// F3.5: an update patches in place rather than rebuilding whenever the new
    /// string measures the same size — roughly 9x cheaper on a large list. An
    /// axis the author fixed cannot move, so a label with an explicit width, a
    /// `VirtualList` row, or a paragraph that still wraps to the same number of
    /// lines always takes the fast path. A change that really would resize the
    /// box rebuilds, so correctness never depends on the guess.
    pub fn bind_text(mut self, d: Dynamic<String>) -> Self {
        self.dyn_text = Some(d);
        self
    }
    /// Bind this node's background colour to a reactive closure (F3) — a
    /// paint-only prop, so a change patches without relayout.
    pub fn bind_background(mut self, d: Dynamic<Color>) -> Self {
        self.dyn_bg = Some(d);
        self
    }
    /// Bind extra classes reactively (F5.2): the closure's `Vec<String>` is
    /// appended to the static classes each build. A change is structural (classes
    /// drive the `.lss` cascade). Use with `bind!`, e.g.
    /// `.bind_class(bind!(|rt| if on.get(rt) { vec!["on".into()] } else { vec![] }))`.
    pub fn bind_class(mut self, d: Dynamic<Vec<String>>) -> Self {
        self.dyn_classes = Some(d);
        self
    }
    /// Set a uniform border (`width` logical px, `color`).
    pub fn border(mut self, color: Color, width: f64) -> Self {
        self.border = Some(lumen_render::Border { width, color });
        self
    }
    /// Set a drop shadow.
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }
    /// Replace the layout style.
    pub fn style(mut self, style: LayoutStyle) -> Self {
        self.style = style;
        self
    }
    /// Set a click handler.
    pub fn on_click(mut self, f: impl Fn(&Runtime) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
    /// Set the drag-and-drop drop handler (T5.2).
    pub fn on_drop(
        mut self,
        f: impl Fn(&Runtime, &lumen_core::events::DropData) + 'static,
    ) -> Self {
        self.on_drop = Some(Rc::new(f));
        self
    }
    /// Set the key handler (fires on this node while it is focused).
    pub fn on_key(mut self, f: impl Fn(&Runtime, &lumen_core::events::KeyEvent) + 'static) -> Self {
        self.on_key = Some(Rc::new(f));
        self
    }
    /// Mark the node keyboard-focusable (so it can receive `on_key`).
    pub fn focusable(mut self) -> Self {
        self.focusable = true;
        self
    }
    /// Set the light-dismiss handler (fires on an outside press or Escape).
    pub fn on_dismiss(mut self, f: impl Fn(&Runtime) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(f));
        self
    }
    /// Set the step-up handler (W2) — pair it with [`Action::Increment`].
    pub fn on_increment(mut self, f: impl Fn(&Runtime) + 'static) -> Self {
        self.on_increment = Some(Rc::new(f));
        self
    }
    /// Set the step-down handler (W2) — pair it with [`Action::Decrement`].
    pub fn on_decrement(mut self, f: impl Fn(&Runtime) + 'static) -> Self {
        self.on_decrement = Some(Rc::new(f));
        self
    }
    /// Set the direct-value handler (W2) — pair it with [`Action::SetValue`].
    pub fn on_set_value(mut self, f: impl Fn(&Runtime, &str) + 'static) -> Self {
        self.on_set_value = Some(Rc::new(f));
        self
    }
    /// Clip descendants to this element's bounds (`overflow: hidden`).
    pub fn clip(mut self, on: bool) -> Self {
        self.clip = on;
        self
    }
    /// Paint this subtree on top of everything (a portal/overlay).
    pub fn overlay(mut self, on: bool) -> Self {
        self.overlay = on;
        self
    }
    /// Replace the children.
    pub fn children(mut self, kids: impl Into<Vec<Element>>) -> Self {
        self.children = kids.into();
        self
    }
}

/// Animation/timer requests a build emitted, collected for the host (the shell
/// schedules the next frame from these; tests read them directly). Re-collected
/// from scratch on every build, so the build closure is the single source of
/// truth (like signals and effects) — a request lives only while it is re-emitted.
#[derive(Default)]
pub struct FrameRequests {
    /// Any node asked to keep animating (redraw continuously).
    pub continuous: bool,
    /// Whether the build read the virtual clock (`now_ms`). If so the frame is a
    /// function of time, so the runtime must rebuild whenever the clock advances —
    /// even for time-driven UI that didn't schedule a `wake_at`/`animate`.
    pub read_clock: bool,
    /// Absolute virtual-clock deadlines (ms) at which the UI wants a frame.
    pub wakes: Vec<f64>,
    /// Background-work spawn requests this build emitted (the data layer). The
    /// runtime dispatches them after the build, on its executor (see `tasks`).
    pub tasks: Vec<TaskRequest>,
    /// C.4b: named app commands registered this build
    /// ([`BuildCx::register_command`]) — the `app.command` agent verb and
    /// future command-palette UI invoke them by name.
    pub commands: Vec<(String, Handler)>,
    /// P.3c: menu model declared this build (`None` = leave the installed
    /// menu untouched).
    pub menu: Option<crate::system::MenuModel>,
}

/// Task factory boxes (M.5): `Send` where threads exist; wasm futures are
/// `!Send` and single-threaded (trait objects only take auto-trait bounds,
/// so the split is on the alias — see `lumen_core::tasks::MaybeSend`).
#[cfg(not(target_arch = "wasm32"))]
pub type BlockingFactory = Box<dyn FnOnce(lumen_core::tasks::Sink) + Send>;
/// wasm: runs inline (no threads).
#[cfg(target_arch = "wasm32")]
pub type BlockingFactory = Box<dyn FnOnce(lumen_core::tasks::Sink)>;
/// Async-work factory: given the [`Sink`](lumen_core::tasks::Sink), yields
/// the future the executor runs.
#[cfg(not(target_arch = "wasm32"))]
pub type FutureFactory =
    Box<dyn FnOnce(lumen_core::tasks::Sink) -> lumen_core::tasks::BoxFuture + Send>;
/// wasm: single-threaded, `!Send` futures welcome.
#[cfg(target_arch = "wasm32")]
pub type FutureFactory = Box<dyn FnOnce(lumen_core::tasks::Sink) -> lumen_core::tasks::BoxFuture>;

/// The work half of a [`TaskRequest`]. Each variant is "given a
/// [`Sink`](lumen_core::tasks::Sink), do the work".
pub enum TaskKind {
    /// CPU-bound work for `spawn_blocking`.
    Blocking(BlockingFactory),
    /// Async work for `spawn` — a factory that, given the sink, yields the future.
    Future(FutureFactory),
}

/// A request to run background work, recorded during build and dispatched by the
/// runtime *after* the build (it owns the executor + the deferred-op channel, so
/// the executor never leaks into `BuildCx`). The runtime mints the sink — bound
/// to `token` — at dispatch, runs the work, and files the backend handle under
/// `id` in the task table.
pub struct TaskRequest {
    pub(crate) id: IdHash,
    pub(crate) token: CancelToken,
    pub(crate) kind: TaskKind,
}

/// A live task's shared record (TC1): the cancel token minted when it was
/// *declared*, and the backend handle attached when it was *dispatched* — which
/// happens after the build that declared it, hence the interior mutability.
pub(crate) struct TaskSlot {
    /// The scope that declared it. The task dies when that scope does; this is
    /// what makes `cx.task` a subscription with a lifetime rather than a
    /// fire-and-forget.
    pub(crate) owner: IdHash,
    token: CancelToken,
    handle: RefCell<Option<Box<dyn TaskHandle>>>,
}

impl TaskSlot {
    fn new(owner: IdHash) -> TaskSlot {
        TaskSlot {
            owner,
            token: CancelToken::new(),
            handle: RefCell::new(None),
        }
    }

    /// The token to bind this task's [`Sink`](lumen_core::tasks::Sink) to.
    pub(crate) fn token(&self) -> CancelToken {
        self.token.clone()
    }

    /// File the backend handle once the executor has been given the work.
    pub(crate) fn attach(&self, handle: Box<dyn TaskHandle>) {
        *self.handle.borrow_mut() = Some(handle);
    }

    /// Stop the task: raise the token — which is what makes it *correct*, since
    /// no write of this task's can land afterwards — then ask the backend to
    /// drop the work if it still owns it.
    pub(crate) fn cancel(&self) {
        self.token.cancel();
        if let Some(h) = self.handle.borrow().as_ref() {
            h.abort();
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
}

impl Drop for TaskSlot {
    /// Teardown backstop: dropping the table (app shutdown) stops its tasks.
    /// Every *deliberate* cancellation calls [`TaskSlot::cancel`] explicitly,
    /// because an escaped [`AbortHandle`] keeps the `Rc` alive past removal.
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Live tasks by identity, owned by `Headless` and threaded into `BuildCx` —
/// the same shape as [`ScopeCache`].
pub(crate) type TaskTable = crate::fxhash::HashMap<IdHash, Rc<TaskSlot>>;

/// A handle to a task started with [`BuildCx::abortable_task`], letting app code
/// stop it early — the "cancel by choice" half of the data layer.
///
/// Cheap to clone and **not** `Send`: it is meant to be captured straight into a
/// [`Handler`], which is `Rc`-based too. That is also the only way to keep one,
/// since a handle can never live in a signal (`.ai_docs/02-spec-core.md` §4
/// forbids OS handles in stored state).
///
/// Aborting is *additional* to the task's scope lifetime, never a replacement:
/// the task still dies with its scope whether or not you hold this.
#[derive(Clone)]
pub struct AbortHandle {
    slot: Rc<TaskSlot>,
    /// Scope-local mirror of "this was aborted", so the UI can *react* to it.
    ///
    /// The cancel token itself is a plain `AtomicBool` that no reactive scope
    /// subscribes to. Cancelling flips no signal, so — since `abort` typically
    /// stops work rather than producing a value — nothing would mark the frame
    /// dirty and the view would keep rendering as if the task were still
    /// running. This signal is what makes the abort observable.
    flag: Signal<bool>,
}

impl AbortHandle {
    pub(crate) fn new(slot: Rc<TaskSlot>, flag: Signal<bool>) -> AbortHandle {
        AbortHandle { slot, flag }
    }

    /// Stop the task. Idempotent; safe to call after it has already finished.
    ///
    /// Writes through its `Sink` stop landing immediately. Whether the *work*
    /// stops depends on the backend — see
    /// [`TaskHandle`]. A blocking job already
    /// running on a pool thread only stops if it polls `sink.is_cancelled()`.
    ///
    /// Takes the runtime like any other state write, because that is what this
    /// is: it schedules the rebuild that lets the view notice.
    pub fn abort(&self, cx: &impl lumen_core::state::WriteCx) {
        self.slot.cancel();
        self.flag.set(cx, true);
    }

    /// Whether this task has been cancelled — by [`abort`](Self::abort), by its
    /// scope dying, or by a newer generation superseding it.
    ///
    /// Reading this during a build subscribes to the abort, so a view that shows
    /// "cancelled" updates on its own.
    pub fn is_aborted(&self, cx: &impl lumen_core::state::ReadCx) -> bool {
        // The signal covers explicit aborts reactively; the token also catches
        // the paths that never run app code (scope death, a superseded
        // generation) — those always coincide with a rebuild happening anyway.
        self.flag.get(cx) || self.slot.is_cancelled()
    }
}

/// A memoized view scope's cached output plus the signals it read (F1). While
/// `reads` stays current (none written since), the subtree is reused verbatim
/// instead of re-running the scope closure.
pub(crate) struct CachedScope {
    reads: lumen_core::state::ReadSet,
    /// Hash of the caller-supplied deps, or `None` for a plain `cx.scope`.
    /// `IdHash` rather than `u64` — the same fold the rest of identity uses.
    ///
    /// A scope invalidates on tracked signal reads — which is exactly nothing
    /// when the closure captures plain data read by its *parent*:
    ///
    /// ```ignore
    /// let items = items_signal.get(rt);            // read HERE, in the parent
    /// cx.scope(("row", i), move |_| row(&items[i]))  // this scope reads nothing
    /// ```
    ///
    /// An empty `ReadSet` is always "current", so that scope would be memo-hit
    /// forever and the row would freeze. `deps` is how the caller says what the
    /// subtree is a function of when it is not a function of any signal.
    deps: Option<IdHash>,
    /// Shared, immutable cached subtree (A.3.2) — hits hand out a stub
    /// holding this `Rc` instead of deep-cloning the tree; `build_node`
    /// either copies the scope's retained per-node work forward or (fallback)
    /// materializes an owned clone to lower normally.
    element: std::rc::Rc<Element>,
}

/// Per-app store of memoized scope subtrees, keyed by scope identity path. Owned
/// by `Headless`, persists across builds, threaded into `BuildCx`.
pub(crate) type ScopeCache = crate::fxhash::HashMap<IdHash, CachedScope>;

/// The build context handed to the root closure and components. Exposes signal
/// creation, the (virtual) clock, time-driven animation, and background tasks.
pub struct BuildCx<'a> {
    rt: &'a Runtime,
    now_ms: f64,
    requests: RefCell<Vec<f64>>,
    continuous: Cell<bool>,
    read_clock: Cell<bool>,
    pub(crate) tasks: RefCell<Vec<TaskRequest>>,
    /// C.4b: named commands registered this build.
    commands: RefCell<Vec<(String, Handler)>>,
    /// P.3c: menu model declared this build (`cx.set_menu`); the app applies
    /// it on pump (change-detected, so an identical model doesn't churn the
    /// native menu).
    menu: RefCell<Option<crate::system::MenuModel>>,
    /// Memoized subtrees (F1), persisted on `Headless` across builds.
    scope_cache: &'a RefCell<ScopeCache>,
    /// Scope keys accessed this build (F5 GC): after the build, cached scopes +
    /// scope-local signals whose key is absent are swept, bounding a churning
    /// keyed list's memory.
    scope_live: &'a RefCell<crate::fxhash::HashSet<IdHash>>,
    /// Scopes that took the memo-hit path this build. Their closures did not
    /// run, so nested `cx.scope` calls inside them never announced themselves in
    /// `scope_live` — yet those children are still on screen, embedded in the
    /// reused subtree. The sweep consults this to tell "skipped" from "gone".
    scope_skipped: &'a RefCell<crate::fxhash::HashSet<IdHash>>,
    /// Live background tasks (TC1), persisted on `Headless` across builds like
    /// `scope_cache`. Declaring a task registers its slot here; the sweep that
    /// drops dead scopes cancels the tasks they own.
    tasks_table: &'a RefCell<TaskTable>,
    /// Identity of the enclosing `scope` ([`ROOT_ID`] at the root). Keys created
    /// inside a scope fold into this, so a reused component gets its own state.
    /// `Copy`, so re-addressing a signal costs no allocation (ADR-021).
    prefix_hash: Cell<IdHash>,
    /// *Readable* name prefix matching `prefix_hash`, for the names snapshots
    /// and agent dep reporting show. Built when a scope **re-runs**, never on a
    /// memo hit — so a skipped subtree allocates nothing.
    prefix: RefCell<String>,
    /// Logical surface size at build time. A resize forces a rebuild, so a view
    /// that materializes only what fits (a virtualized grid) can read this to
    /// size its viewport and reveal more content as the window grows.
    size: lumen_core::geometry::Size,
}

impl<'a> BuildCx<'a> {
    pub(crate) fn new(
        rt: &'a Runtime,
        now_ms: f64,
        scope_cache: &'a RefCell<ScopeCache>,
        scope_live: &'a RefCell<crate::fxhash::HashSet<IdHash>>,
        scope_skipped: &'a RefCell<crate::fxhash::HashSet<IdHash>>,
        tasks_table: &'a RefCell<TaskTable>,
        size: lumen_core::geometry::Size,
    ) -> BuildCx<'a> {
        BuildCx {
            rt,
            now_ms,
            requests: RefCell::new(Vec::new()),
            continuous: Cell::new(false),
            read_clock: Cell::new(false),
            tasks: RefCell::new(Vec::new()),
            commands: RefCell::new(Vec::new()),
            menu: RefCell::new(None),
            scope_cache,
            scope_live,
            scope_skipped,
            tasks_table,
            prefix_hash: Cell::new(ROOT_ID),
            prefix: RefCell::new(String::new()),
            size,
        }
    }

    /// The logical surface size at build time (a resize triggers a rebuild, so
    /// reading this keeps a virtualized view sized to the current window).
    pub fn size(&self) -> lumen_core::geometry::Size {
        self.size
    }

    /// Whether the pointer is currently over the node with stable id `id`.
    ///
    /// Lets a widget *build* hover-dependent structure — a tooltip, a
    /// reveal-on-hover row action — instead of only restyling through `.lss
    /// :hovered`.
    ///
    /// **Signal-backed on purpose.** Visual state is normally applied *after*
    /// the view closures run, precisely so pointer motion gets memoized
    /// rebuilds (`tests/hover_memo.rs`). Reading hover through a signal keeps
    /// that property honest: only the scopes that actually call this record the
    /// dependency and re-run, while the rest stay memoized. Reading it any
    /// other way would let a memoized subtree go stale.
    pub fn is_hovered(&self, id: &str) -> bool {
        let cur: Signal<String> = self.rt.signal(HOVER_SIGNAL, String::new);
        cur.with(self.rt, |h| h == id)
    }

    /// Create or re-attach a signal keyed by `key` (02 §4), namespaced under the
    /// enclosing [`scope`](Self::scope) so a reused component gets its own state.
    ///
    /// `key` is anything `Hash + Debug` (ADR-021): a `&str`, an index, or a
    /// typed key like `Field::Row(id)`. Re-addressing an existing signal
    /// allocates nothing, so per-item state in a list is cheap to rebuild every
    /// frame.
    pub fn signal<T: State, K: Hash + Debug>(&self, key: K, init: impl FnOnce() -> T) -> Signal<T> {
        let owner = self.prefix_hash.get();
        self.rt.signal_at(
            fold_id(owner, hash_id(&key)),
            owner,
            || self.scoped_name(&key),
            init,
        )
    }

    /// SD6b: [`signal`](Self::signal) through a typed
    /// [`SignalKey<T>`](lumen_core::state::SignalKey), so a key's value type is
    /// fixed where the key is declared rather than re-asserted at every use.
    pub fn signal_keyed<T: State>(
        &self,
        key: lumen_core::state::SignalKey<T>,
        init: impl FnOnce() -> T,
    ) -> Signal<T> {
        self.signal(key, init)
    }

    /// Register a freshly-declared task under `key` (TC1), returning the id to
    /// stamp on its [`TaskRequest`] and the slot the dispatcher fills in.
    ///
    /// The identity is `fold_id(scope, key)` — exactly what [`signal`](Self::signal)
    /// uses, so a task and its tracker signal share one identity and die together.
    /// Any previous generation at that identity is cancelled here: this is what
    /// stops a deps change from leaving two tasks writing the same signal.
    pub(crate) fn register_task<K: Hash>(&self, key: &K) -> (IdHash, Rc<TaskSlot>) {
        let owner = self.prefix_hash.get();
        let id = fold_id(owner, hash_id(key));
        let slot = Rc::new(TaskSlot::new(owner));
        // Cancel the superseded generation *explicitly*: an escaped
        // `AbortHandle` keeps its `Rc` alive past removal, so relying on the
        // `Drop` impl here would silently leave the old task running.
        if let Some(prev) = self.tasks_table.borrow_mut().insert(id, Rc::clone(&slot)) {
            prev.cancel();
        }
        (id, slot)
    }

    /// The slot of an already-running task at `key` in the current scope.
    pub(crate) fn lookup_task<K: Hash>(&self, key: &K) -> Option<Rc<TaskSlot>> {
        let id = fold_id(self.prefix_hash.get(), hash_id(key));
        self.tasks_table.borrow().get(&id).cloned()
    }

    /// A memoized view region (F1). Runs `f` inside a read-tracking window and
    /// caches the subtree it returns; on a later build the closure is **skipped**
    /// (the cached subtree reused) while none of the signals it read has changed.
    /// `id` must be unique among sibling scopes (like a signal key; use an
    /// explicit index in a loop). Turns the store's fine-grained reactivity into
    /// fine-grained *view* updates: a write re-runs only the scopes that read it
    /// (and their ancestors, whose subtrees embed them).
    ///
    /// Scopes that emit a frame-request (read the clock, `animate`, `wake_*`) are
    /// never cached — they re-run every build, as they must.
    ///
    /// A scope that *spawns* a task is uncacheable only on the build that
    /// actually spawns: `cx.task` pushes a request just once per `(key, deps)`,
    /// so once the task is running the scope becomes cacheable again and is
    /// memo-skipped like any other. That is deliberate and costs nothing —
    /// a task's lifetime is tied to its scope being *live* (recorded below
    /// before the cache check), not to its declaration re-running.
    pub fn scope<K: Hash + Debug>(
        &mut self,
        id: K,
        f: impl FnOnce(&mut BuildCx) -> Element,
    ) -> Element {
        self.scope_impl(id, None, f)
    }

    /// A memoized region that is also a function of `deps` — the form to use
    /// when the subtree is built from plain data rather than from signals.
    ///
    /// [`scope`](Self::scope) alone invalidates on the signals its closure
    /// *reads*. A closure that reads none — because its data was read by the
    /// parent and captured — has an empty read set, which is always current, so
    /// it would be memo-hit forever and render frozen content. Passing the data
    /// (or a version of it) as `deps` states the dependency the read tracker
    /// cannot see, exactly as [`task`](Self::task) does for background work.
    ///
    /// ```ignore
    /// let items = items.get(cx.runtime());
    /// cx.scope_with_deps(("row", i), &items[i], move |_| row(&items[i]))
    /// ```
    ///
    /// Changing `deps` re-runs the closure and keeps the scope's identity, so
    /// scope-local signals and tasks survive — unlike folding the deps into the
    /// `id`, which would create a *new* scope and shed them.
    pub fn scope_with_deps<K: Hash + Debug, D: Hash>(
        &mut self,
        id: K,
        deps: D,
        f: impl FnOnce(&mut BuildCx) -> Element,
    ) -> Element {
        self.scope_impl(id, Some(hash_id(&deps)), f)
    }

    fn scope_impl<K: Hash + Debug>(
        &mut self,
        id: K,
        deps: Option<IdHash>,
        f: impl FnOnce(&mut BuildCx) -> Element,
    ) -> Element {
        let parent = self.prefix_hash.get();
        let key = fold_id(parent, hash_id(&id));
        self.scope_live.borrow_mut().insert(key);
        // The memo-hit path needs identity only — no name, no allocation. This
        // is the steady state for an unchanged list row.
        if let Some(el) = self.cached_if_current(key, deps) {
            // The closure did not run, so any `cx.scope` nested inside it never
            // reached the `scope_live` insert above — even though those children
            // are still on screen inside the reused subtree. Note the skip so the
            // F5 sweep does not shed their state (or cancel their tasks); walking
            // the subtree here instead would put an O(scopes) cost on the very
            // path memoization exists to make cheap.
            self.scope_skipped.borrow_mut().insert(key);
            return el;
        }
        // Re-run: establish this scope's identity + name prefix, collect its
        // reads, and note whether it emitted any frame-request (⇒ not cacheable).
        let rt = self.rt.clone();
        rt.note_scope(key, parent);
        self.prefix_hash.set(key);
        let prev = self.prefix.replace(format!("{}/", self.scoped_name(&id)));
        let before = self.request_fingerprint();
        let (mut element, reads) = rt.collect_reads(|| f(self));
        let cacheable = self.request_fingerprint() == before;
        self.prefix.replace(prev);
        self.prefix_hash.set(parent);
        // Project the scope's signal dependencies onto its subtree root, for
        // observability (F2) — the agent sees why this subtree updates.
        element.scope_deps = Some(reads.dep_keys(self.rt));
        // A.3.1: tag the root with its scope key so `build_node` records the
        // node span (cached clones inherit the tag).
        element.scope_key = Some(key);
        if cacheable {
            self.scope_cache.borrow_mut().insert(
                key,
                CachedScope {
                    reads,
                    deps,
                    element: std::rc::Rc::new(element.clone()),
                },
            );
        } else {
            self.scope_cache.borrow_mut().remove(&key);
        }
        element
    }

    /// The cached subtree for `key` if its recorded deps are all still current.
    /// A skipped scope replays its deps into the enclosing collectors so they
    /// still count as structural (F1 × F3.4) — otherwise a change to a memoized
    /// scope's signal would go unnoticed.
    fn cached_if_current(&self, key: IdHash, deps: Option<IdHash>) -> Option<Element> {
        let cache = self.scope_cache.borrow();
        let cached = cache.get(&key)?;
        if cached.deps == deps && cached.reads.is_current(self.rt) {
            self.rt.replay_reads(&cached.reads);
            // A.3.2: hand out a lightweight stub — an `Rc` bump, not a deep
            // clone. `build_node` resolves it (copy-forward or materialize).
            Some(Element {
                scope_key: Some(key),
                shared: Some(std::rc::Rc::clone(&cached.element)),
                ..Element::default()
            })
        } else {
            None
        }
    }

    /// Register a named app command (C.4b, 02 §4): a `Fn(&Runtime)` the
    /// agent (`app.command {name}`) — and future command-palette UI — can
    /// invoke without geometry. Re-registered per build like handlers;
    /// last registration of a name wins.
    pub fn register_command(&mut self, name: &str, f: impl Fn(&Runtime) + 'static) {
        self.commands
            .borrow_mut()
            .push((name.to_string(), std::rc::Rc::new(f)));
    }

    /// Declare the app's native menu (P.3c, 02 §4): a [`MenuModel`]
    /// (crate::system::MenuModel) whose items may carry accelerator chords
    /// (`MenuItem::accel("Ctrl+O")`). Menu ids double as command names —
    /// bind behavior with [`register_command`](Self::register_command) under
    /// the same id. Applied on pump only when the model actually changed.
    pub fn set_menu(&mut self, menu: crate::system::MenuModel) {
        *self.menu.borrow_mut() = Some(menu);
    }

    /// Derived value (02 §4, W.3): recomputed when its reads change,
    /// notifying subscribers only when the value actually changes
    /// (`PartialEq`). Keyed like a signal — the enclosing `cx.scope`
    /// prefixes `name`.
    pub fn memo<T: PartialEq + lumen_core::state::State, K: Hash + Debug>(
        &self,
        key: K,
        f: impl Fn(&lumen_core::state::ReadScope) -> T + 'static,
    ) -> lumen_core::state::Memo<T> {
        let owner = self.prefix_hash.get();
        self.rt.memo_at(
            fold_id(owner, hash_id(&key)),
            owner,
            || self.scoped_name(&key),
            f,
        )
    }

    /// Register (or replace) an effect (02 §4, W.3): re-runs whenever any
    /// signal it read changes; runs once immediately to establish
    /// subscriptions. Keyed like a signal.
    pub fn effect<K: Hash>(&self, key: K, f: impl Fn(&lumen_core::state::ReadScope) + 'static) {
        self.rt
            .effect_at(fold_id(self.prefix_hash.get(), hash_id(&key)), f)
    }

    /// The readable name for `key` under the enclosing scope — what snapshots
    /// and agent dep reporting display.
    ///
    /// Only ever called on the cold path (a key seen for the first time), which
    /// is what keeps re-addressing allocation-free.
    fn scoped_name<K: Debug + ?Sized>(&self, key: &K) -> String {
        let p = self.prefix.borrow();
        let name = key_name(key);
        if p.is_empty() {
            name
        } else {
            format!("{p}{name}")
        }
    }

    /// A cheap fingerprint of the frame-requests emitted so far; if a scope
    /// changes it, the scope is time/task-dependent and must not be memoized.
    fn request_fingerprint(&self) -> (bool, bool, usize, usize) {
        (
            self.continuous.get(),
            self.read_clock.get(),
            self.requests.borrow().len(),
            self.tasks.borrow().len(),
        )
    }

    /// The reactive runtime (for reading/writing signals during build).
    pub fn runtime(&self) -> &Runtime {
        self.rt
    }

    /// The current virtual-clock time in milliseconds (for time-driven UI).
    /// Reading it marks the frame time-dependent, so the runtime rebuilds on every
    /// clock advance (even without an explicit `animate`/`wake_at`).
    pub fn now_ms(&self) -> f64 {
        self.read_clock.set(true);
        self.now_ms
    }

    /// Request continuous animation: the host should keep producing frames (each
    /// advancing the virtual clock) as long as this is re-emitted. Use for UI
    /// whose value is a function of [`now_ms`](Self::now_ms) (a spinner, a clock
    /// hand). Idle and deterministic: nothing animates unless a build asks.
    pub fn animate(&self) {
        self.continuous.set(true);
    }

    /// Request a single frame at virtual time `t_ms` (absolute). Lets time-based
    /// state transitions (a toast auto-dismiss, a delayed reveal) happen without
    /// other input. A past `t_ms` is ignored by the host.
    pub fn wake_at(&self, t_ms: f64) {
        self.requests.borrow_mut().push(t_ms);
    }

    /// Request a single frame `dt_ms` from now (relative form of [`wake_at`](Self::wake_at)).
    pub fn wake_in(&self, dt_ms: f64) {
        self.wake_at(self.now_ms + dt_ms);
    }

    /// Take the animation/timer/task requests this build emitted.
    pub(crate) fn take_requests(self) -> FrameRequests {
        FrameRequests {
            continuous: self.continuous.get(),
            read_clock: self.read_clock.get(),
            wakes: self.requests.into_inner(),
            tasks: self.tasks.into_inner(),
            commands: self.commands.into_inner(),
            menu: self.menu.into_inner(),
        }
    }
}
