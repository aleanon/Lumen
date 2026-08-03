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
    el: Element,
    editor: Signal<TextEditor>,
    mirror: Signal<String>,
    /// Rebuilt lazily by the option setters, which have to re-derive the shown
    /// text (masking, placeholder) and the caret position that goes with it.
    text: String,
    mask: Option<char>,
    placeholder: Option<String>,
    /// Caret position in *characters* (for remapping onto masked glyphs) and in
    /// bytes (the unmasked case).
    caret_chars: usize,
    caret_byte: usize,
}

impl TextInput {
    /// A text input with `initial` contents, state stored under `name`.
    pub fn new(cx: &BuildCx, name: &str, initial: &str) -> TextInput {
        let editor = cx.signal(name, || TextEditor::new(initial));
        let mirror = cx.signal(mirror_key(name), || initial.to_string());
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
            // W2: `SetValue` is declared, so implement it — replace the whole
            // contents (the AT/agent meaning of setting a field's value).
            on_set_value: Some(Rc::new(move |rt, v| {
                // Replace the contents via select-all + insert so the edit goes
                // through the normal path (undo history stays coherent).
                editor.update(rt, |e| {
                    e.select_all();
                    e.insert(v);
                });
                sync_mirror(rt, editor, mirror);
            })),
            on_key: Some(Rc::new(move |rt, ke| {
                edit_key(rt, ke, editor, mirror, false);
            })),
            ..Element::default()
        };
        let caret_byte = ed.cursor();
        let caret_chars = text
            .get(..caret_byte)
            .map(|s| s.chars().count())
            .unwrap_or_else(|| text.chars().count());
        TextInput {
            el,
            editor,
            mirror,
            text,
            mask: None,
            placeholder: None,
            caret_chars,
            caret_byte,
        }
    }

    /// Re-derive the displayed content after an option changed.
    ///
    /// Masking and the placeholder both change what is *shown* without changing
    /// what is *stored*, so this is the one place that decides the glyphs, the
    /// text colour, and whether a caret is drawn.
    fn refresh(&mut self) {
        let empty = self.text.is_empty();
        let shown = match (&self.mask, empty, &self.placeholder) {
            // Placeholder only shows while genuinely empty, and never leaks the
            // masked value.
            (_, true, Some(p)) if !p.is_empty() => p.clone(),
            (Some(c), _, _) => std::iter::repeat_n(*c, self.text.chars().count()).collect(),
            (None, true, _) => " ".to_string(),
            (None, false, _) => self.text.clone(),
        };
        let showing_placeholder = empty && self.placeholder.as_ref().is_some_and(|p| !p.is_empty());
        let mut ts = lumen_text::TextStyle::default();
        if showing_placeholder {
            ts.color = lumen_core::Color::srgb8(0x8a, 0x90, 0x9a, 0xff);
        }
        self.el.content = NodeContent::Text(shown, ts);
        // A masked field's caret must be remapped: `caret_byte` indexes the
        // SHOWN text, and one plaintext byte is not one bullet.
        self.el.caret_byte = Some(match self.mask {
            Some(c) => self.caret_chars * c.len_utf8(),
            None if showing_placeholder => 0,
            None => self.caret_byte,
        });
        // Selection highlighting would leak the shape of a masked value's
        // sub-ranges; drop it while masked.
        if self.mask.is_some() {
            self.el.selection = None;
        }
        // Semantics: never publish the secret. An *empty* masked field has no
        // secret, so its placeholder still labels it (that is the whole point
        // of a placeholder on a password box).
        if showing_placeholder {
            self.el.label = self.placeholder.clone().unwrap_or_default();
            self.el.value = Some(String::new());
        } else if let Some(c) = self.mask {
            self.el.label = String::new();
            self.el.value = Some(std::iter::repeat_n(c, self.text.chars().count()).collect());
        } else {
            self.el.label = self.text.clone();
            self.el.value = Some(self.text.clone());
        }
    }

    /// Show `text` when the field is empty (and report it as the accessible
    /// label, the way a placeholder should).
    pub fn placeholder(mut self, text: impl Into<String>) -> TextInput {
        self.placeholder = Some(text.into());
        self.refresh();
        self
    }

    /// Cap the contents at `n` characters. Input past the cap is dropped
    /// (the field keeps what it already had) rather than truncating silently
    /// mid-edit.
    pub fn max_length(mut self, n: usize) -> TextInput {
        let editor = self.editor;
        let mirror = self.mirror;
        self.el.on_text = Some(Rc::new(move |rt, t| {
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

    /// Make the field read-only: it still focuses, selects and copies, but
    /// cannot be edited.
    ///
    /// Distinct from `.disabled(true)` (W1), which removes it from input and
    /// focus entirely — read-only content is still readable and selectable,
    /// which is what assistive tech expects from `Readonly`.
    pub fn read_only(mut self, yes: bool) -> TextInput {
        if yes {
            self.el.on_text = None;
            self.el.on_set_value = None;
            let editor = self.editor;
            // Keep caret movement, selection and copy; drop the mutating keys.
            self.el.on_key = Some(Rc::new(move |rt, ke| {
                edit_key_readonly(rt, ke, editor);
            }));
            self.el.states.push(lumen_core::semantics::State::Readonly);
        }
        self
    }

    /// Run `f` with the full contents after every edit.
    pub fn on_change(mut self, f: impl Fn(&Runtime, &str) + 'static) -> TextInput {
        let editor = self.editor;
        let mirror = self.mirror;
        let prev = self.el.on_text.take();
        self.el.on_text = Some(Rc::new(move |rt, t| {
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

    /// Mask the contents with `bullet` — a password field.
    ///
    /// The stored value is unchanged; only the glyphs and the published
    /// semantics are masked, so the secret never reaches the semantic tree, a
    /// screenshot, or assistive tech.
    pub fn password(mut self, bullet: char) -> TextInput {
        self.mask = Some(bullet);
        self.refresh();
        self
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
        {
            let sig: Signal<String> = rt.signal(mirror_key(name), String::new);
            sig.get(rt)
        }
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
/// The read-only key subset: caret movement, selection and copy — everything
/// that reads, nothing that writes.
pub(crate) fn edit_key_readonly(rt: &Runtime, ke: &KeyEvent, editor: Signal<TextEditor>) {
    let ctrl = ke.modifiers.contains(Modifiers::CTRL) || ke.modifiers.contains(Modifiers::META);
    let shift = ke.modifiers.contains(Modifiers::SHIFT);
    match &ke.key {
        Key::Named(NamedKey::ArrowLeft) => editor.update(rt, |e| e.move_left(shift)),
        Key::Named(NamedKey::ArrowRight) => editor.update(rt, |e| e.move_right(shift)),
        Key::Named(NamedKey::Home) => editor.update(rt, |e| e.move_home(shift)),
        Key::Named(NamedKey::End) => editor.update(rt, |e| e.move_end(shift)),
        Key::Character(s) if ctrl => match s.to_lowercase().as_str() {
            "a" => editor.update(rt, |e| e.select_all()),
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
