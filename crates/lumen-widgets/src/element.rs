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
use std::rc::Rc;

/// A click/activate handler. Re-registered every build; never stored (ADR-013).
pub type Handler = Rc<dyn Fn(&Runtime)>;
/// A wheel handler receiving the vertical delta (logical px).
pub type WheelHandler = Rc<dyn Fn(&Runtime, f64)>;
/// A drag handler receiving the pointer's fraction along the node's main axis.
pub type DragHandler = Rc<dyn Fn(&Runtime, f64)>;
/// A committed-text handler (text inputs).
pub type TextHandler = Rc<dyn Fn(&Runtime, &str)>;

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
    /// Text content + its style.
    pub text: Option<(String, TextStyle)>,
    /// Whether the node is keyboard-focusable.
    pub focusable: bool,
    /// Whether the node is elided from semantics (pure layout).
    pub elide_semantics: bool,
    /// Explicit semantic states (e.g. checked/disabled).
    pub states: Vec<SemState>,
    /// Scroll info for scroll containers (semantics).
    pub scroll: Option<ScrollInfo>,
    /// Image content (the Image widget).
    pub image: Option<RgbaImage>,
    /// Click handler.
    pub on_click: Option<Handler>,
    /// Wheel handler (scroll containers).
    pub on_wheel: Option<WheelHandler>,
    /// Drag handler (sliders); receives the fraction along the main axis.
    pub on_drag: Option<DragHandler>,
    /// Committed-text handler (text inputs).
    pub on_text: Option<TextHandler>,
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
            text: None,
            focusable: false,
            elide_semantics: false,
            states: Vec::new(),
            scroll: None,
            image: None,
            on_click: None,
            on_wheel: None,
            on_drag: None,
            on_text: None,
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
            text: Some((s, TextStyle::default())),
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
            text: Some((
                label,
                TextStyle {
                    font_size: 16.0,
                    color: Color::WHITE,
                },
            )),
            ..Element::default()
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
    /// Replace the children.
    pub fn children(mut self, kids: impl Into<Vec<Element>>) -> Self {
        self.children = kids.into();
        self
    }
}

/// The build context handed to the root closure and components. Exposes signal
/// creation and the (virtual) clock.
pub struct BuildCx<'a> {
    rt: &'a Runtime,
    now_ms: f64,
}

impl<'a> BuildCx<'a> {
    pub(crate) fn new(rt: &'a Runtime, now_ms: f64) -> BuildCx<'a> {
        BuildCx { rt, now_ms }
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
}
