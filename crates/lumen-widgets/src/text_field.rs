//! [`TextField`] — a self-stateful **multi-line** editable area. Its `Element` is
//! built inside [`TextField::new`]; the editor state lives in a `Signal<TextEditor>`
//! keyed by `name` (with a `"{name}.text"` string mirror), exactly like
//! [`TextInput`](crate::TextInput). Enter inserts a newline; Up/Down move the
//! caret between visual lines. Read the text with [`TextInput::text_of`](crate::TextInput::text_of).

use crate::element::NodeContent;
use crate::text_input::edit_key;
use crate::widget::{impl_widget, Common, Widget};
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
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, TextField, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, TextField::new(cx, "bio", "Multi-line text…").into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 300.0, 130.0, "text_field");
/// ```
///
/// Renders:
///
/// ![Text Field example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/text_field.png)
///
/// The picture above is `src/doc_shots/text_field.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct TextField {
    /// The editor's text at build time, and its caret/selection.
    text: String,
    caret: usize,
    selection: Option<(usize, usize)>,
    editor: lumen_core::state::Signal<TextEditor>,
    mirror: lumen_core::state::Signal<String>,
    /// Visible height in lines, and the box width in px.
    lines: u32,
    width: f32,
    common: Common,
}

/// The default visible height, in lines.
const LINES: u32 = 5;
/// One line of the default text style, px.
const LINE_H: f32 = 20.0;

impl TextField {
    /// A multi-line editable area; the editor lives in a `Signal<TextEditor>`
    /// keyed by `name`, with a `"{name}.text"` string mirror.
    pub fn new(cx: &BuildCx, name: &str, initial: &str) -> TextField {
        let editor = cx.signal(name, || TextEditor::new(initial));
        let mirror = cx.signal(format!("{name}.text"), || initial.to_string());
        let ed = editor.get(cx.runtime());
        TextField {
            text: ed.text().to_string(),
            caret: ed.cursor(),
            selection: ed.has_selection().then(|| ed.selection()),
            editor,
            mirror,
            lines: LINES,
            width: 260.0,
            common: Common::default(),
        }
    }

    /// Visible height, in lines of text.
    pub fn lines(mut self, n: u32) -> TextField {
        self.lines = n;
        self
    }

    /// Box width in px.
    pub fn width(mut self, px: f32) -> TextField {
        self.width = px;
        self
    }
}

impl Widget for TextField {
    fn build(self) -> Element {
        let TextField {
            text,
            caret,
            selection,
            editor,
            mirror,
            lines,
            width,
            common,
        } = self;
        let shown = if text.is_empty() {
            " ".to_string()
        } else {
            text.clone()
        };
        let mut el = Element {
            role: Role::TextInput,
            focusable: true,
            // An I-beam over anything typeable.
            cursor: Some(lumen_core::CursorShape::Text),
            label: text.clone(),
            value: Some(text),
            actions: vec![Action::Focus, Action::SetValue],
            background: Some(Color::srgb8(0xf7, 0xf8, 0xfa, 0xff)),
            corner_radius: 6.0,
            // Matches `TextInput`: a filled box with no edge reads as a label.
            border: Some(lumen_render::Border {
                width: 1.0,
                color: Color::srgb8(0xc9, 0xd0, 0xdb, 0xff),
            }),
            style: LayoutStyle {
                padding: Edges::all(Dim::px(8.0)),
                min_width: Dim::px(220.0),
                // Several lines tall, and a fixed width so the text wraps
                // (multi-line). Height grows past this via min_height semantics.
                min_height: Dim::px(LINE_H * lines as f32 + 16.0),
                width: Dim::px(width),
                ..LayoutStyle::default()
            },
            content: NodeContent::Text(shown, TextStyle::default()),
            caret_byte: Some(caret),
            selection,
            on_text: Some(Rc::new(move |rt, t| {
                editor.update(rt, |e| e.insert(t));
                let text = editor.get(rt).text().to_string();
                mirror.set(rt, text);
            })),
            on_caret_set: Some(Rc::new(move |rt, byte, extend| {
                editor.update(rt, |e| e.place(byte, extend));
            })),
            // SD4/W2: `SetValue` is declared above, so it must be implemented.
            // It was not — `input.invokeAction {action:"setValue"}` and any AT
            // offering "set value" on a TextField silently did nothing. The
            // gap survived because W0106, the diagnostic that names exactly
            // this, was unreachable from `App::lint()` until SD4 wired it in.
            // Same shape as TextInput: select-all + insert, so the edit goes
            // through the normal path and undo history stays coherent.
            on_set_value: Some(Rc::new(move |rt, v| {
                editor.update(rt, |e| {
                    e.select_all();
                    e.insert(v);
                });
                let text = editor.get(rt).text().to_string();
                mirror.set(rt, text);
            })),
            // Multi-line: Enter inserts a newline (Up/Down are handled app-side).
            on_key: Some(Rc::new(move |rt, ke| {
                edit_key(rt, ke, editor, mirror, true);
            })),
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(TextField);
