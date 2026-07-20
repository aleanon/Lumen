//! [`TextField`] — a self-stateful **multi-line** editable area. Its `Element` is
//! built inside [`TextField::new`]; the editor state lives in a `Signal<TextEditor>`
//! keyed by `name` (with a `"{name}.text"` string mirror), exactly like
//! [`TextInput`](crate::TextInput). Enter inserts a newline; Up/Down move the
//! caret between visual lines. Read the text with [`TextInput::text_of`](crate::TextInput::text_of).

use crate::element::NodeContent;
use crate::text_input::edit_key;
use crate::widget::impl_common;
use crate::{BuildCx, Element};
use lumen_core::semantics::{Action, Role};
use lumen_core::Color;
use lumen_layout::{Dim, Edges, LayoutStyle};
use lumen_text::{TextEditor, TextStyle};
use std::rc::Rc;

/// A multi-line text area with a caret, selection, clipboard, and undo. Wraps to
/// its width; the box is sized for several lines.
/// # Example
///
/// ```
/// use lumen_widgets::{App, TextField};
///
/// let app = App::new(|cx| TextField::new(cx, "bio", "Multi-line text…").into());
/// # lumen_widgets::doc_shot(app, 280.0, 120.0, "text_field");
/// ```
pub struct TextField {
    el: Element,
}

impl TextField {
    /// A multi-line field with `initial` contents, state stored under `name`.
    /// Defaults to ~5 visible lines; override with [`lines`](TextField::lines)
    /// or [`width`](TextField::width).
    pub fn new(cx: &BuildCx, name: &str, initial: &str) -> TextField {
        let editor = cx.signal(name, || TextEditor::new(initial));
        let mirror = cx.signal(&format!("{name}.text"), || initial.to_string());
        let ed = editor.get(cx.runtime());
        let text = ed.text().to_string();
        let shown = if text.is_empty() {
            " ".to_string()
        } else {
            text.clone()
        };
        let line_h = 20.0_f32;
        let el = Element {
            role: Role::TextInput,
            focusable: true,
            label: text.clone(),
            value: Some(text.clone()),
            actions: vec![Action::Focus, Action::SetValue],
            background: Some(Color::srgb8(0xf2, 0xf2, 0xf2, 0xff)),
            corner_radius: 6.0,
            style: LayoutStyle {
                padding: Edges::all(Dim::px(8.0)),
                min_width: Dim::px(220.0),
                // Several lines tall, and a fixed width so the text wraps
                // (multi-line). Height grows past this via min_height semantics.
                min_height: Dim::px(line_h * 5.0 + 16.0),
                width: Dim::px(260.0),
                ..LayoutStyle::default()
            },
            content: NodeContent::Text(shown, TextStyle::default()),
            caret_byte: Some(ed.cursor()),
            selection: ed.has_selection().then(|| ed.selection()),
            on_text: Some(Rc::new(move |rt, t| {
                editor.update(rt, |e| e.insert(t));
                let text = editor.get(rt).text().to_string();
                mirror.set(rt, text);
            })),
            on_caret_set: Some(Rc::new(move |rt, byte, extend| {
                editor.update(rt, |e| e.place(byte, extend));
            })),
            // Multi-line: Enter inserts a newline (Up/Down are handled app-side).
            on_key: Some(Rc::new(move |rt, ke| {
                edit_key(rt, ke, editor, mirror, true);
            })),
            ..Element::default()
        };
        TextField { el }
    }

    /// Set the visible line count (≈ rows of `20px`).
    pub fn lines(mut self, n: u32) -> TextField {
        self.el.style.min_height = Dim::px(20.0 * n as f32 + 16.0);
        self
    }

    /// Set the wrap width in px.
    pub fn width(mut self, px: f32) -> TextField {
        self.el.style.width = Dim::px(px);
        self
    }
}

impl_common!(TextField);
