//! [`Button`] — a push button. Its `Element` is built inside [`Button::new`].

use crate::element::NodeContent;
use crate::widget::{impl_widget, Common, Widget};
use crate::{Element, Handler};
use lumen_core::semantics::{Action, Role};
use lumen_core::state::Runtime;
use lumen_core::Color;
use lumen_layout::{Dim, Edges, LayoutStyle};
use lumen_text::TextStyle;

/// A push button labelled with text. Accent (primary) styling by default; use
/// [`ghost`](Button::ghost) for a quiet variant and [`on_press`](Button::on_press)
/// for the handler.
///
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Button, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Button::new("Save").on_press(|_| {}).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 160.0, 72.0, "button");
/// ```
///
/// Renders:
///
/// ![Button example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/button.png)
///
/// The picture above is `src/doc_shots/button.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Button {
    /// The label, kept as `Text` so a reactive binding survives to `build`.
    label: crate::Text,
    /// The press handler, already `Rc`-wrapped (one allocation either way).
    on_press: Option<Handler>,
    /// Which of the two emphases to paint.
    emphasis: Emphasis,
    /// A label colour that overrides the emphasis's own.
    text_color: Option<Color>,
    common: Common,
}

/// A button's visual weight. Held as a one-byte tag rather than as a background
/// colour already written into a node, so `.ghost()` after `.primary()` costs a
/// tag write instead of overwriting a fill (and a `TextStyle` lookup) that was
/// only just set.
#[derive(Clone, Copy, PartialEq)]
enum Emphasis {
    /// Accent fill, white label — the default.
    Primary,
    /// Quiet grey fill, dark label.
    Ghost,
}

impl Emphasis {
    /// `(fill, label)` for this emphasis.
    fn colors(self) -> (Color, Color) {
        match self {
            Emphasis::Primary => (Color::srgb8(0x1a, 0x73, 0xe8, 0xff), Color::WHITE),
            Emphasis::Ghost => (
                Color::srgb8(0xe9, 0xeb, 0xef, 0xff),
                Color::srgb8(0x1f, 0x23, 0x29, 0xff),
            ),
        }
    }
}

impl Button {
    /// A button labelled `label`.
    pub fn new(label: impl Into<crate::Text>) -> Button {
        Button {
            label: label.into(),
            on_press: None,
            emphasis: Emphasis::Primary,
            text_color: None,
            common: Common::default(),
        }
    }

    /// Run `f` when the button is pressed.
    pub fn on_press(mut self, f: impl Fn(&Runtime) + 'static) -> Button {
        self.on_press = Some(std::rc::Rc::new(f));
        self
    }

    /// Accent (primary) emphasis — the default, but explicit reads clearly.
    pub fn primary(mut self) -> Button {
        self.emphasis = Emphasis::Primary;
        self
    }

    /// Set the label colour (independent of `primary`/`ghost`).
    ///
    /// Order-independent now: it is applied over the emphasis at build time, so
    /// `.text_color(c).ghost()` and `.ghost().text_color(c)` agree.
    pub fn text_color(mut self, c: Color) -> Button {
        self.text_color = Some(c);
        self
    }

    /// Quiet (ghost) emphasis.
    pub fn ghost(mut self) -> Button {
        self.emphasis = Emphasis::Ghost;
        self
    }
}

impl Button {
    /// Decompose for the `direct` prototype (WT-EXP), with the emphasis already
    /// resolved to `(fill, ink)` — the same resolution `build` performs.
    #[doc(hidden)]
    pub fn into_parts(self) -> (crate::Text, Option<Handler>, Color, Color, Common) {
        let (fill, ink) = self.emphasis.colors();
        let ink = self.text_color.unwrap_or(ink);
        (self.label, self.on_press, fill, ink, self.common)
    }
}

impl Widget for Button {
    fn build(self) -> Element {
        let Button {
            label,
            on_press,
            emphasis,
            text_color,
            common,
        } = self;
        let (label, dyn_text) = label.into_parts();
        let (fill, ink) = emphasis.colors();
        let mut el = Element {
            role: Role::Button,
            label: label.clone(),
            dyn_text,
            actions: vec![Action::Click, Action::Focus],
            focusable: true,
            // A hand over anything clickable: the affordance users read
            // without thinking.
            cursor: Some(lumen_core::CursorShape::Pointer),
            background: Some(fill),
            corner_radius: 8.0,
            on_click: on_press,
            style: LayoutStyle {
                padding: Edges {
                    left: Dim::px(16.0),
                    right: Dim::px(16.0),
                    top: Dim::px(9.0),
                    bottom: Dim::px(9.0),
                },
                ..LayoutStyle::default()
            },
            content: NodeContent::Text(
                label,
                TextStyle {
                    font_size: 15.0,
                    weight: 600.0,
                    color: text_color.unwrap_or(ink),
                    line_height: None,
                    letter_spacing: 0.0,
                    family: None,
                    features: None,
                    variations: None,
                    italic: false,
                    align: Default::default(),
                },
            ),
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Button);
