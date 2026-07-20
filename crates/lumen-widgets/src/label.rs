//! [`Label`] — a static text run. Its `Element` is built inside [`Label::new`].

use crate::element::NodeContent;
use crate::widget::impl_common;
use crate::Element;
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_text::TextStyle;

/// A line (or wrapped paragraph, if given a width) of text. Exposes typography
/// modifiers; no event handlers.
/// # Example
///
/// ```
/// use lumen_widgets::{App, Label};
///
/// let app = App::new(|_| Label::new("Hello, Lumen").into());
/// # lumen_widgets::doc_shot(app, 140.0, 36.0, "label");
/// ```
pub struct Label {
    el: Element,
}

impl Label {
    /// A label showing `s`.
    pub fn new(s: impl Into<String>) -> Label {
        let s = s.into();
        let el = Element {
            role: Role::Text,
            label: s.clone(),
            content: NodeContent::Text(s, TextStyle::default()),
            ..Element::default()
        };
        Label { el }
    }

    /// Font size in logical px.
    pub fn size(mut self, px: f32) -> Label {
        if let Some(ts) = self.el.text_style_mut() {
            ts.font_size = px;
        }
        self
    }

    /// Font weight (100–900).
    pub fn weight(mut self, w: f32) -> Label {
        if let Some(ts) = self.el.text_style_mut() {
            ts.weight = w;
        }
        self
    }

    /// Bold (weight 700).
    pub fn bold(self) -> Label {
        self.weight(700.0)
    }

    /// Text colour.
    pub fn color(mut self, c: Color) -> Label {
        if let Some(ts) = self.el.text_style_mut() {
            ts.color = c;
        }
        self
    }

    /// Shape with a registered font family by name (see `App::with_font` /
    /// `TextEngine::register_font`); unknown names fall back to the default font.
    pub fn family(mut self, name: impl Into<String>) -> Label {
        if let Some(ts) = self.el.text_style_mut() {
            ts.family = Some(name.into());
        }
        self
    }

    /// Line height as a multiple of font size.
    pub fn line_height(mut self, multiple: f32) -> Label {
        if let Some(ts) = self.el.text_style_mut() {
            ts.line_height = Some(multiple);
        }
        self
    }

    /// Extra letter tracking, px.
    pub fn letter_spacing(mut self, px: f32) -> Label {
        if let Some(ts) = self.el.text_style_mut() {
            ts.letter_spacing = px;
        }
        self
    }

    /// Wrap to `px` wide (a fixed width turns the label into a paragraph).
    pub fn width(mut self, px: f32) -> Label {
        self.el.style.width = lumen_layout::Dim::px(px);
        self
    }
}

impl_common!(Label);
