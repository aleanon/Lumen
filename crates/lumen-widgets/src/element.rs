//! Element descriptions and the build context.
//!
//! For M0 an [`Element`] is a concrete description (the full `Widget` trait
//! arrives in T0.10). It carries everything the headless runtime needs to lay
//! out, paint, route events, and emit semantics for one node.

use lumen_core::semantics::{Action, Role, ScrollInfo, State as SemState};
use lumen_core::state::{Runtime, State};
use lumen_core::{Color, Signal, StableId};
use lumen_layout::{Dim, Display, FlexDirection, LayoutStyle};
use lumen_render::RgbaImage;
use lumen_text::TextStyle;
use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

/// A click/activate handler. Re-registered every build; never stored (ADR-013).
pub type Handler = Rc<dyn Fn(&Runtime)>;
/// A wheel handler receiving the vertical delta (logical px).
pub type WheelHandler = Rc<dyn Fn(&Runtime, f64)>;
/// A drag handler receiving the pointer's fraction along the node's width and
/// height (`frac_x`, `frac_y`), each clamped to `0.0..=1.0`. Horizontal controls
/// (sliders, the pane-grid split) use `frac_x`; vertical ones (a scrollbar) use
/// `frac_y`.
pub type DragHandler = Rc<dyn Fn(&Runtime, f64, f64)>;
/// A committed-text handler (text inputs).
pub type TextHandler = Rc<dyn Fn(&Runtime, &str)>;
/// A key handler on the focused node, receiving each `KeyDown` (the node decides
/// what to do — e.g. a list handling PageUp/Down/Home/End/arrows).
pub type KeyHandler = Rc<dyn Fn(&Runtime, &lumen_core::events::KeyEvent)>;
/// A drop handler receiving the dropped payload (T5.2 drag-and-drop).
pub type DropHandler = Rc<dyn Fn(&Runtime, &lumen_core::events::DropData)>;
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
    /// Corner radius (px).
    pub corner_radius: f64,
    /// Leaf content — text, image, or canvas, mutually exclusive (E1).
    pub content: NodeContent,
    /// Whether the node is keyboard-focusable.
    pub focusable: bool,
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
    /// Light-dismiss handler: fired when a pointer press lands *outside* this
    /// element's bounds, or on Escape. Used for click-away on transient overlays
    /// (dropdowns, popovers, menus, tooltips).
    pub on_dismiss: Option<Handler>,
    /// Clip descendants to this element's (rounded) bounds — `overflow: hidden`.
    /// Used by scroll viewports so off-screen content doesn't paint outside.
    pub clip: bool,
    /// Optional drop shadow behind the box.
    pub shadow: Option<Shadow>,
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
            corner_radius: 0.0,
            content: NodeContent::None,
            focusable: false,
            elide_semantics: false,
            states: Vec::new(),
            scroll: None,
            on_click: None,
            on_wheel: None,
            on_drag: None,
            on_drop: None,
            on_text: None,
            on_key: None,
            on_dismiss: None,
            clip: false,
            shadow: None,
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
    /// Set the background fill.
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
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
    /// Clip descendants to this element's bounds (`overflow: hidden`).
    pub fn clip(mut self, on: bool) -> Self {
        self.clip = on;
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
    /// Absolute virtual-clock deadlines (ms) at which the UI wants a frame.
    pub wakes: Vec<f64>,
    /// Background-work spawn requests this build emitted (the data layer). The
    /// runtime dispatches them after the build, on its executor (see `tasks`).
    pub tasks: Vec<TaskRequest>,
}

/// A request to run background work, recorded during build and dispatched by the
/// runtime *after* the build (it owns the executor + the deferred-op channel, so
/// the executor never leaks into `BuildCx`). Each variant is "given a [`Sink`](lumen_core::tasks::Sink),
/// do the work" — the runtime mints the sink at dispatch and runs it.
pub enum TaskRequest {
    /// CPU-bound work for `spawn_blocking`.
    Blocking(Box<dyn FnOnce(lumen_core::tasks::Sink) + Send>),
    /// Async work for `spawn` — a factory that, given the sink, yields the future.
    Future(Box<dyn FnOnce(lumen_core::tasks::Sink) -> lumen_core::tasks::BoxFuture + Send>),
}

/// The build context handed to the root closure and components. Exposes signal
/// creation, the (virtual) clock, time-driven animation, and background tasks.
pub struct BuildCx<'a> {
    rt: &'a Runtime,
    now_ms: f64,
    requests: RefCell<Vec<f64>>,
    continuous: Cell<bool>,
    pub(crate) tasks: RefCell<Vec<TaskRequest>>,
}

impl<'a> BuildCx<'a> {
    pub(crate) fn new(rt: &'a Runtime, now_ms: f64) -> BuildCx<'a> {
        BuildCx {
            rt,
            now_ms,
            requests: RefCell::new(Vec::new()),
            continuous: Cell::new(false),
            tasks: RefCell::new(Vec::new()),
        }
    }

    /// Create or re-attach a signal keyed by `name` (02 §4).
    pub fn signal<T: State>(&self, name: &str, init: impl FnOnce() -> T) -> Signal<T> {
        self.rt.signal(name, init)
    }

    /// The reactive runtime (for reading/writing signals during build).
    pub fn runtime(&self) -> &Runtime {
        self.rt
    }

    /// The current virtual-clock time in milliseconds (for time-driven UI).
    pub fn now_ms(&self) -> f64 {
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
            wakes: self.requests.into_inner(),
            tasks: self.tasks.into_inner(),
        }
    }
}
