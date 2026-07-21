//! [`Button`] — a push button. Its `Element` is built inside [`Button::new`].

use crate::element::NodeContent;
use crate::widget::impl_common;
use crate::Element;
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
/// use lumen_widgets::{App, Button};
///
/// let app = App::new(|_| Button::new("Save").on_press(|_| {}).into());
/// # lumen_widgets::doc_shot(app, 160.0, 60.0, "button");
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
    el: Element,
}

impl Button {
    /// A button labelled `label`.
    pub fn new(label: impl Into<String>) -> Button {
        let label = label.into();
        let el = Element {
            role: Role::Button,
            label: label.clone(),
            actions: vec![Action::Click, Action::Focus],
            focusable: true,
            background: Some(Color::srgb8(0x1a, 0x73, 0xe8, 0xff)),
            corner_radius: 8.0,
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
                    color: Color::WHITE,
                    line_height: None,
                    letter_spacing: 0.0,
                    family: None,
                },
            ),
            ..Element::default()
        };
        Button { el }
    }

    /// Run `f` when the button is pressed.
    pub fn on_press(mut self, f: impl Fn(&Runtime) + 'static) -> Button {
        self.el = self.el.on_click(f);
        self
    }

    /// Accent (primary) emphasis — the default, but explicit reads clearly.
    pub fn primary(mut self) -> Button {
        self.el.background = Some(Color::srgb8(0x1a, 0x73, 0xe8, 0xff));
        if let Some(ts) = self.el.text_style_mut() {
            ts.color = Color::WHITE;
            ts.weight = 600.0;
        }
        self
    }

    /// Set the label colour (independent of `primary`/`ghost`).
    pub fn text_color(mut self, c: Color) -> Button {
        if let Some(ts) = self.el.text_style_mut() {
            ts.color = c;
        }
        self
    }

    /// Quiet (ghost) emphasis.
    pub fn ghost(mut self) -> Button {
        self.el.background = Some(Color::srgb8(0xe9, 0xeb, 0xef, 0xff));
        if let Some(ts) = self.el.text_style_mut() {
            ts.color = Color::srgb8(0x1f, 0x23, 0x29, 0xff);
            ts.weight = 600.0;
        }
        self
    }
}

impl_common!(Button);
