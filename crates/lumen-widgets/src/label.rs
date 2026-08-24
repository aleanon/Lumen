//! [`Label`] — a static text run. Its `Element` is built inside [`Label::new`].

use crate::element::NodeContent;
use crate::widget::{impl_widget, Common, Widget};
use crate::Element;
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_text::TextStyle;

/// A line (or wrapped paragraph, if given a width) of text. Exposes typography
/// modifiers; no event handlers.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Label, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Label::new("Hello, Lumen").into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 160.0, 56.0, "label");
/// ```
///
/// Renders:
///
/// ![Label example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/label.png)
///
/// The picture above is `src/doc_shots/label.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Label {
    /// The run, kept as `Text` so a reactive binding survives to `build`.
    text: crate::Text,
    /// The typography, accumulated directly.
    ///
    /// A label's data really *is* a `TextStyle`, so it is held whole rather than
    /// as a dozen `Option`s. The saving over the old model is that `.size()`,
    /// `.bold()` and `.color()` now write a field each, instead of re-matching
    /// `NodeContent::Text` on a built node to borrow the style back out.
    style: TextStyle,
    /// Wrap width, if the caller turned the label into a paragraph.
    width: Option<f32>,
    common: Common,
}

impl Label {
    /// A label showing `s`.
    pub fn new(s: impl Into<crate::Text>) -> Label {
        Label {
            text: s.into(),
            style: TextStyle::default(),
            width: None,
            common: Common::default(),
        }
    }

    /// Font size in logical px.
    pub fn size(mut self, px: f32) -> Label {
        self.style.font_size = px;
        self
    }

    /// Font weight (100–900).
    pub fn weight(mut self, w: f32) -> Label {
        self.style.weight = w;
        self
    }

    /// Bold (weight 700).
    pub fn bold(self) -> Label {
        self.weight(700.0)
    }

    /// Text colour.
    pub fn color(mut self, c: Color) -> Label {
        self.style.color = c;
        self
    }

    /// Shape with a registered font family by name (see `App::with_font` /
    /// `TextEngine::register_font`); unknown names fall back to the default font.
    pub fn family(mut self, name: impl Into<String>) -> Label {
        self.style.family = Some(name.into());
        self
    }

    /// Line height as a multiple of font size.
    pub fn line_height(mut self, multiple: f32) -> Label {
        self.style.line_height = Some(multiple);
        self
    }

    /// Extra letter tracking, px.
    pub fn letter_spacing(mut self, px: f32) -> Label {
        self.style.letter_spacing = px;
        self
    }

    /// Wrap to `px` wide (a fixed width turns the label into a paragraph).
    pub fn width(mut self, px: f32) -> Label {
        self.width = Some(px);
        self
    }
}

impl Widget for Label {
    fn build(self) -> Element {
        let Label {
            text,
            style,
            width,
            common,
        } = self;
        let (s, dyn_text) = text.into_parts();
        let mut el = Element {
            role: Role::Text,
            label: s.clone(),
            content: NodeContent::Text(s, style),
            dyn_text,
            ..Element::default()
        };
        if let Some(px) = width {
            el.style.width = lumen_layout::Dim::px(px);
        }
        common.apply(&mut el);
        el
    }
}

impl_widget!(Label);
