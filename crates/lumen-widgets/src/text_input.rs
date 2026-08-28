//! [`TextInput`] — a self-stateful single-line editable field. Its `Element` is
//! built inside [`TextInput::new`]; the editor state (text + cursor + selection +
//! undo) lives in a `Signal<TextEditor>` keyed by `name`, with a plain-string
//! mirror under `"{name}.text"` for external readers (see [`TextInput::text_of`]).
//! Supports caret placement, selection, clipboard, undo/redo, and an
//! [`on_submit`](TextInput::on_submit) handler that fires on Enter.

use crate::element::NodeContent;
use crate::widget::{impl_widget, Common, Widget};
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
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, TextInput, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, TextInput::new(cx, "name", "Ada").into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 220.0, 60.0, "text_input");
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
    editor: Signal<TextEditor>,
    mirror: Signal<String>,
    /// The plaintext at build time, plus the caret in both byte and char
    /// units — a masked field needs the char count, because one plaintext
    /// byte is not one bullet.
    text: String,
    caret_byte: usize,
    caret_chars: usize,
    selection: Option<(usize, usize)>,
    mask: Option<char>,
    placeholder: Option<String>,
    /// Set by `max_length` / `on_change`, which compose over each other.
    on_text: Option<crate::element::TextHandler>,
    /// Set by `on_submit`.
    on_key: Option<crate::element::KeyHandler>,
    read_only: bool,
    common: Common,
}

impl TextInput {
    /// A single-line field; the editor lives in a `Signal<TextEditor>` keyed by
    /// `name`, with a `"{name}.text"` string mirror.
    pub fn new(cx: &BuildCx, name: &str, initial: &str) -> TextInput {
        let editor = cx.signal(name, || TextEditor::new(initial));
        let mirror = cx.signal(mirror_key(name), || initial.to_string());
        let ed = editor.get(cx.runtime());
        let text = ed.text().to_string();
        let caret_byte = ed.cursor();
        let caret_chars = text
            .get(..caret_byte)
            .map(|s| s.chars().count())
            .unwrap_or_else(|| text.chars().count());
        TextInput {
            editor,
            mirror,
            text,
            caret_byte,
            caret_chars,
            selection: ed.has_selection().then(|| ed.selection()),
            mask: None,
            placeholder: None,
            on_text: None,
            on_key: None,
            read_only: false,
            common: Common::default(),
        }
    }

    /// Grey prompt text shown while the field is empty.
    ///
    /// A plain field write now. The eager version re-ran `refresh()` here — and
    /// again in `password()` — re-deriving the shown string, the caret mapping,
    /// the label and the value every time either was called.
    pub fn placeholder(mut self, text: impl Into<String>) -> TextInput {
        self.placeholder = Some(text.into());
        self
    }

    /// Cap the field at `n` characters.
    pub fn max_length(mut self, n: usize) -> TextInput {
        let editor = self.editor;
        let mirror = self.mirror;
        self.on_text = Some(Rc::new(move |rt, t| {
            editor.update(rt, |e| {
                let room = n.saturating_sub(e.text().chars().count());
                if room == 0 {
                    return;
                }
                let take: String = t.chars().take(room).collect();
                if !take.is_empty() {
                    e.insert(&take);
                }
            });
            sync_mirror(rt, editor, mirror);
        }));
        self
    }

    /// Make the field readable and selectable but not editable.
    ///
    /// Applied last when the widget lowers, so it wins over a handler set
    /// afterwards in the chain — a read-only field that a later `.on_change()`
    /// quietly made writable again was the eager model's order trap.
    pub fn read_only(mut self, yes: bool) -> TextInput {
        self.read_only = yes;
        self
    }

    /// Run `f` with the field's full text after every edit.
    pub fn on_change(mut self, f: impl Fn(&Runtime, &str) + 'static) -> TextInput {
        let editor = self.editor;
        let mirror = self.mirror;
        let prev = self.on_text.take();
        self.on_text = Some(Rc::new(move |rt, t| {
            match &prev {
                Some(h) => h(rt, t),
                None => {
                    editor.update(rt, |e| e.insert(t));
                    sync_mirror(rt, editor, mirror);
                }
            }
            let now = editor.with(rt, |e| e.text().to_string());
            f(rt, &now);
        }));
        self
    }

    /// Mask the contents with `bullet` (a password field).
    pub fn password(mut self, bullet: char) -> TextInput {
        self.mask = Some(bullet);
        self
    }

    /// Run `f` with the field's text when Enter is pressed, then clear it.
    pub fn on_submit(mut self, f: impl Fn(&Runtime, &str) + 'static) -> TextInput {
        let editor = self.editor;
        let mirror = self.mirror;
        self.on_key = Some(Rc::new(move |rt, ke| {
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

    /// Read the field's current text from outside a build.
    pub fn text_of(rt: &Runtime, name: &str) -> String {
        let sig: Signal<String> = rt.signal(mirror_key(name), String::new);
        sig.get(rt)
    }
}

impl Widget for TextInput {
    fn build(self) -> Element {
        let TextInput {
            editor,
            mirror,
            text,
            caret_byte,
            caret_chars,
            selection,
            mask,
            placeholder,
            on_text,
            on_key,
            read_only,
            common,
        } = self;

        // Derived once, where the eager model re-derived it per modifier.
        let empty = text.is_empty();
        let showing_placeholder = empty && placeholder.as_ref().is_some_and(|p| !p.is_empty());
        let shown = match (&mask, empty, &placeholder) {
            // Placeholder only shows while genuinely empty, and never leaks the
            // masked value.
            (_, true, Some(p)) if !p.is_empty() => p.clone(),
            (Some(c), _, _) => std::iter::repeat_n(*c, text.chars().count()).collect(),
            (None, true, _) => " ".to_string(),
            (None, false, _) => text.clone(),
        };
        let mut ts = TextStyle::default();
        if showing_placeholder {
            ts.color = Color::srgb8(0x8a, 0x90, 0x9a, 0xff);
        }

        // Semantics: never publish the secret. An *empty* masked field has no
        // secret, so its placeholder still labels it (that is the whole point
        // of a placeholder on a password box).
        let (label, value) = if showing_placeholder {
            (placeholder.unwrap_or_default(), String::new())
        } else if let Some(c) = mask {
            (
                String::new(),
                std::iter::repeat_n(c, text.chars().count()).collect(),
            )
        } else {
            (text.clone(), text.clone())
        };

        let mut el = Element {
            role: Role::TextInput,
            focusable: true,
            // An I-beam over anything typeable.
            cursor: Some(lumen_core::CursorShape::Text),
            label,
            value: Some(value),
            actions: vec![Action::Focus, Action::SetValue],
            background: Some(Color::srgb8(0xf7, 0xf8, 0xfa, 0xff)),
            corner_radius: 6.0,
            // A filled box with no edge reads as a label, not a field — you
            // cannot see where it starts or ends against a light page. The
            // hairline is what says "you may type here".
            border: Some(lumen_render::Border {
                width: 1.0,
                color: Color::srgb8(0xc9, 0xd0, 0xdb, 0xff),
            }),
            style: LayoutStyle {
                padding: Edges::all(Dim::px(8.0)),
                min_width: Dim::px(140.0),
                ..LayoutStyle::default()
            },
            content: NodeContent::Text(shown, ts),
            // A masked field's caret must be remapped: `caret_byte` indexes the
            // SHOWN text, and one plaintext byte is not one bullet.

            // Selection highlighting would leak the shape of a masked value's
            // sub-ranges; drop it while masked.

            // W2: `SetValue` is declared, so implement it — replace the whole
            // contents (the AT/agent meaning of setting a field's value).
            ..Element::default()
        }
        .set_selection(if mask.is_some() { None } else { selection })
        .set_caret_byte(Some(match mask {
            Some(c) => caret_chars * c.len_utf8(),
            None if showing_placeholder => 0,
            None => caret_byte,
        }))
        .set_on_set_value(Some(Rc::new(move |rt, v| {
            // Replace the contents via select-all + insert so the edit goes
            // through the normal path (undo history stays coherent).
            editor.update(rt, |e| {
                e.select_all();
                e.insert(v);
            });
            sync_mirror(rt, editor, mirror);
        })))
        .set_on_caret_set(Some(Rc::new(move |rt, byte, extend| {
            editor.update(rt, |e| e.place(byte, extend));
        })))
        .set_on_key(Some(on_key.unwrap_or_else(|| {
            Rc::new(move |rt, ke| {
                edit_key(rt, ke, editor, mirror, false);
            })
        })))
        .set_on_text(Some(on_text.unwrap_or_else(|| {
            Rc::new(move |rt, t| {
                editor.update(rt, |e| e.insert(t));
                sync_mirror(rt, editor, mirror);
            })
        })));

        if read_only {
            el.rare_mut().on_text = None;
            el.rare_mut().on_set_value = None;
            // Keep caret movement, selection and copy; drop the mutating keys.
            el.rare_mut().on_key = Some(Rc::new(move |rt, ke| {
                edit_key_readonly(rt, ke, editor);
            }));
            // Stop advertising the action that was just removed — the list is a
            // contract the agent and assistive tech read (W0106).
            el.actions
                .retain(|a| *a != lumen_core::semantics::Action::SetValue);
            el.states.push(lumen_core::semantics::State::Readonly);
        }

        common.apply(&mut el);
        el
    }
}

impl_widget!(TextInput);

/// The plain-string mirror signal key for a field named `name`.
fn mirror_key(name: &str) -> String {
    format!("{name}.text")
}

/// Re-publish the editor's committed text to the string mirror after an edit.
fn sync_mirror(rt: &Runtime, editor: Signal<TextEditor>, mirror: Signal<String>) {
    let text = editor.get(rt).text().to_string();
    mirror.set(rt, text);
}

/// The editing key map, shared by every text widget in the framework.
///
/// | keys | effect |
/// |---|---|
/// | ←/→ | caret by one grapheme |
/// | Ctrl/Cmd+←/→ | caret by one **word** |
/// | Home / End | start / end of the **line** (of the buffer, single-line) |
/// | Ctrl/Cmd+Home / End | start / end of the buffer |
/// | Backspace / Delete | one grapheme, or the selection |
/// | Ctrl/Cmd+Backspace / Delete | one **word**, or the selection |
/// | Ctrl/Cmd+A | select all |
/// | Ctrl/Cmd+L | select the current **line** |
/// | Ctrl/Cmd+C / X / V | copy / cut / paste |
/// | Ctrl/Cmd+Z / Y | undo / redo |
/// | Shift + any motion | extends the selection instead of collapsing it |
/// | Enter (multiline) | inserts a newline |
///
/// Plain character input arrives separately through `on_text`; a chord never
/// produces text, because the shell drops `text` while Ctrl/Cmd is down.
/// Vertical nav (Up/Down) is handled app-side — it needs layout geometry.
/// Keeps the string mirror in sync.
/// The read-only key subset: caret movement, selection and copy — everything
/// in the map above that reads, nothing that writes.
pub(crate) fn edit_key_readonly(rt: &Runtime, ke: &KeyEvent, editor: Signal<TextEditor>) {
    let ctrl = ke.modifiers.contains(Modifiers::CTRL) || ke.modifiers.contains(Modifiers::META);
    let shift = ke.modifiers.contains(Modifiers::SHIFT);
    match &ke.key {
        Key::Named(NamedKey::ArrowLeft) if ctrl => editor.update(rt, |e| e.move_word_left(shift)),
        Key::Named(NamedKey::ArrowRight) if ctrl => editor.update(rt, |e| e.move_word_right(shift)),
        Key::Named(NamedKey::ArrowLeft) => editor.update(rt, |e| e.move_left(shift)),
        Key::Named(NamedKey::ArrowRight) => editor.update(rt, |e| e.move_right(shift)),
        Key::Named(NamedKey::Home) if ctrl => editor.update(rt, |e| e.move_home(shift)),
        Key::Named(NamedKey::End) if ctrl => editor.update(rt, |e| e.move_end(shift)),
        Key::Named(NamedKey::Home) => editor.update(rt, |e| e.move_line_home(shift)),
        Key::Named(NamedKey::End) => editor.update(rt, |e| e.move_line_end(shift)),
        Key::Character(s) if ctrl => match s.to_lowercase().as_str() {
            "a" => editor.update(rt, |e| e.select_all()),
            "l" => editor.update(rt, |e| e.select_line()),
            "c" => rt.set_clipboard(editor.get(rt).selected_text()),
            _ => {}
        },
        _ => {}
    }
}

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
        Key::Named(NamedKey::Backspace) if ctrl => editor.update(rt, |e| e.delete_word_left()),
        Key::Named(NamedKey::Delete) if ctrl => editor.update(rt, |e| e.delete_word_right()),
        Key::Named(NamedKey::Backspace) => editor.update(rt, |e| e.backspace()),
        Key::Named(NamedKey::Delete) => editor.update(rt, |e| e.delete()),
        Key::Named(NamedKey::ArrowLeft) if ctrl => editor.update(rt, |e| e.move_word_left(shift)),
        Key::Named(NamedKey::ArrowRight) if ctrl => editor.update(rt, |e| e.move_word_right(shift)),
        Key::Named(NamedKey::ArrowLeft) => editor.update(rt, |e| e.move_left(shift)),
        Key::Named(NamedKey::ArrowRight) => editor.update(rt, |e| e.move_right(shift)),
        // Ctrl jumps to the ends of the buffer; bare Home/End stay on the line,
        // which is the same key in a single-line field and the useful one in a
        // multi-line field.
        Key::Named(NamedKey::Home) if ctrl => editor.update(rt, |e| e.move_home(shift)),
        Key::Named(NamedKey::End) if ctrl => editor.update(rt, |e| e.move_end(shift)),
        Key::Named(NamedKey::Home) => editor.update(rt, |e| e.move_line_home(shift)),
        Key::Named(NamedKey::End) => editor.update(rt, |e| e.move_line_end(shift)),
        Key::Named(NamedKey::Enter) if multiline => editor.update(rt, |e| e.insert("\n")),
        Key::Character(s) if ctrl => match s.to_lowercase().as_str() {
            "a" => editor.update(rt, |e| e.select_all()),
            "l" => editor.update(rt, |e| e.select_line()),
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
