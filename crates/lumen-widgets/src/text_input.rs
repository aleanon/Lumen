//! [`TextInput`] — a self-stateful single-line text field. Its `Element` is built
//! inside [`TextInput::new`]; the value lives in a signal keyed by `name`, so it
//! survives rebuilds.

use crate::element::NodeContent;
use crate::widget::impl_common;
use crate::{BuildCx, Element};
use lumen_core::semantics::{Action, Role};
use lumen_core::Color;
use lumen_layout::{Dim, Edges, LayoutStyle};
use lumen_text::TextStyle;
use std::rc::Rc;

/// A single-line text input. Committed text is appended to the value signal
/// (`name`); full IME/editing is the text stack's concern.
pub struct TextInput {
    el: Element,
}

impl TextInput {
    /// A text input with `initial` contents, state stored under `name`.
    pub fn new(cx: &BuildCx, name: &str, initial: &str) -> TextInput {
        let value = cx.signal(name, || initial.to_string());
        let v = value.get(cx.runtime());
        // Keep a non-empty glyph box so an empty field still lays out with height.
        let shown = if v.is_empty() {
            " ".to_string()
        } else {
            v.clone()
        };
        let el = Element {
            role: Role::TextInput,
            focusable: true,
            label: v.clone(),
            value: Some(v),
            actions: vec![Action::Focus, Action::SetValue],
            background: Some(Color::srgb8(0xf2, 0xf2, 0xf2, 0xff)),
            corner_radius: 6.0,
            style: LayoutStyle {
                padding: Edges::all(Dim::px(8.0)),
                min_width: Dim::px(140.0),
                ..LayoutStyle::default()
            },
            content: NodeContent::Text(shown, TextStyle::default()),
            on_text: Some(Rc::new(move |rt, t| {
                let t = t.to_string();
                value.update(rt, |s| s.push_str(&t))
            })),
            ..Element::default()
        };
        TextInput { el }
    }
}

impl_common!(TextInput);
