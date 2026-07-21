//! [`TextInput`] — a self-stateful single-line editable field. Its `Element` is
//! built inside [`TextInput::new`]; the editor state (text + cursor + selection +
//! undo) lives in a `Signal<TextEditor>` keyed by `name`, with a plain-string
//! mirror under `"{name}.text"` for external readers (see [`TextInput::text_of`]).
//! Supports caret placement, selection, clipboard, undo/redo, and an
//! [`on_submit`](TextInput::on_submit) handler that fires on Enter.

use crate::element::NodeContent;
use crate::widget::impl_common;
use crate::{BuildCx, Element};
use lumen_core::events::{Key, KeyEvent, Modifiers, NamedKey};
use lumen_core::semantics::{Action, Role};
use lumen_core::state::{Runtime, Signal};
use lumen_core::Color;
use lumen_layout::{Dim, Edges, LayoutStyle};
use lumen_text::{TextEditor, TextStyle};
use std::rc::Rc;

/// A single-line text input with a caret, selection, clipboard, and undo.
/// # Example
///
/// ```
/// use lumen_widgets::{App, TextInput};
///
/// let app = App::new(|cx| TextInput::new(cx, "name", "Ada").into());
/// # lumen_widgets::doc_shot(app, 200.0, 44.0, "text_input");
/// ```
///
/// Renders:
///
/// ![Text Input example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/text_input.png)
///
/// The picture above is `src/doc_shots/text_input.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct TextInput {
    el: Element,
    editor: Signal<TextEditor>,
    mirror: Signal<String>,
}

impl TextInput {
    /// A text input with `initial` contents, state stored under `name`.
    pub fn new(cx: &BuildCx, name: &str, initial: &str) -> TextInput {
        let editor = cx.signal(name, || TextEditor::new(initial));
        let mirror = cx.signal(&mirror_key(name), || initial.to_string());
        let ed = editor.get(cx.runtime());
        let text = ed.text().to_string();
        // Keep a non-empty glyph box so an empty field still lays out with height.
        let shown = if text.is_empty() {
            " ".to_string()
        } else {
            text.clone()
        };
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
                min_width: Dim::px(140.0),
                ..LayoutStyle::default()
            },
            content: NodeContent::Text(shown, TextStyle::default()),
            caret_byte: Some(ed.cursor()),
            selection: ed.has_selection().then(|| ed.selection()),
            on_text: Some(Rc::new(move |rt, t| {
                editor.update(rt, |e| e.insert(t));
                sync_mirror(rt, editor, mirror);
            })),
            on_caret_set: Some(Rc::new(move |rt, byte, extend| {
                editor.update(rt, |e| e.place(byte, extend));
            })),
            on_key: Some(Rc::new(move |rt, ke| {
                edit_key(rt, ke, editor, mirror, false);
            })),
            ..Element::default()
        };
        TextInput { el, editor, mirror }
    }

    /// Run `f` with the current value when Enter is pressed, then clear the field
    /// (the "submit a line" pattern). All other editing keys still apply.
    pub fn on_submit(mut self, f: impl Fn(&Runtime, &str) + 'static) -> TextInput {
        let editor = self.editor;
        let mirror = self.mirror;
        self.el.on_key = Some(Rc::new(move |rt, ke| {
            if matches!(ke.key, Key::Named(NamedKey::Enter)) {
                let v = editor.get(rt).text().to_string();
                if !v.is_empty() {
                    f(rt, &v);
                    editor.set(rt, TextEditor::new(""));
                    mirror.set(rt, String::new());
                }
            } else {
                edit_key(rt, ke, editor, mirror, false);
            }
        }));
        self
    }

    /// The current text of the field named `name`, readable from any handler
    /// (e.g. a sibling button) without holding the `TextInput`'s signal handle.
    pub fn text_of(rt: &Runtime, name: &str) -> String {
        rt.signal::<String>(&mirror_key(name), String::new).get(rt)
    }
}

impl_common!(TextInput);

/// The plain-string mirror signal key for a field named `name`.
fn mirror_key(name: &str) -> String {
    format!("{name}.text")
}

/// Re-publish the editor's committed text to the string mirror after an edit.
fn sync_mirror(rt: &Runtime, editor: Signal<TextEditor>, mirror: Signal<String>) {
    let text = editor.get(rt).text().to_string();
    mirror.set(rt, text);
}

/// Apply one `KeyDown` to the editor signal: navigation (arrows/Home/End, with
/// Shift extending the selection), deletion (Backspace/Delete), clipboard
/// (Ctrl/Cmd+C/X/V), select-all (Ctrl/Cmd+A), undo/redo (Ctrl/Cmd+Z/Y), and —
/// when `multiline` — Enter inserts a newline. Plain character input arrives
/// separately via `on_text`. Vertical nav (Up/Down) is handled app-side (it
/// needs layout geometry). Keeps the string mirror in sync.
pub(crate) fn edit_key(
    rt: &Runtime,
    ke: &KeyEvent,
    editor: Signal<TextEditor>,
    mirror: Signal<String>,
    multiline: bool,
) {
    let ctrl = ke.modifiers.contains(Modifiers::CTRL) || ke.modifiers.contains(Modifiers::META);
    let shift = ke.modifiers.contains(Modifiers::SHIFT);
    let mut changed = true;
    match &ke.key {
        Key::Named(NamedKey::Backspace) => editor.update(rt, |e| e.backspace()),
        Key::Named(NamedKey::Delete) => editor.update(rt, |e| e.delete()),
        Key::Named(NamedKey::ArrowLeft) => editor.update(rt, |e| e.move_left(shift)),
        Key::Named(NamedKey::ArrowRight) => editor.update(rt, |e| e.move_right(shift)),
        Key::Named(NamedKey::Home) => editor.update(rt, |e| e.move_home(shift)),
        Key::Named(NamedKey::End) => editor.update(rt, |e| e.move_end(shift)),
        Key::Named(NamedKey::Enter) if multiline => editor.update(rt, |e| e.insert("\n")),
        Key::Character(s) if ctrl => match s.to_lowercase().as_str() {
            "a" => editor.update(rt, |e| e.select_all()),
            "c" => {
                rt.set_clipboard(editor.get(rt).selected_text());
                changed = false;
            }
            "x" => {
                let cut = editor.get(rt).selected_text();
                if !cut.is_empty() {
                    rt.set_clipboard(cut);
                    editor.update(rt, |e| {
                        e.cut();
                    });
                } else {
                    changed = false;
                }
            }
            "v" => {
                let clip = rt.clipboard();
                editor.update(rt, |e| e.paste(&clip));
            }
            "z" => editor.update(rt, |e| e.undo()),
            "y" => editor.update(rt, |e| e.redo()),
            _ => changed = false,
        },
        _ => changed = false,
    }
    if changed {
        sync_mirror(rt, editor, mirror);
    }
}
