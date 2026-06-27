//! The text editing model (T1.5): cursor/selection, insert/delete, undo/redo,
//! IME pre-edit, and clipboard. Byte offsets index into the buffer; navigation
//! moves by `char` boundaries (good enough pre-grapheme-segmentation).

/// IME pre-edit (composition) state overlaid on the buffer at the cursor.
#[derive(Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Preedit {
    /// The composing text.
    pub text: String,
    /// Optional cursor/selection within the preedit (byte offsets).
    pub cursor: Option<(usize, usize)>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Snapshot {
    text: String,
    cursor: usize,
    anchor: usize,
}

/// A single-document text editor.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TextEditor {
    text: String,
    cursor: usize,
    anchor: usize,
    preedit: Option<Preedit>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl TextEditor {
    /// A new editor over `initial`, cursor at the end.
    pub fn new(initial: &str) -> TextEditor {
        let end = initial.len();
        TextEditor {
            text: initial.to_string(),
            cursor: end,
            anchor: end,
            preedit: None,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// The committed buffer (no preedit).
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The cursor byte offset.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The selection as `(start, end)` byte offsets (start ≤ end).
    pub fn selection(&self) -> (usize, usize) {
        (self.cursor.min(self.anchor), self.cursor.max(self.anchor))
    }

    /// Whether any text is selected.
    pub fn has_selection(&self) -> bool {
        self.cursor != self.anchor
    }

    /// The selected substring.
    pub fn selected_text(&self) -> String {
        let (a, b) = self.selection();
        self.text[a..b].to_string()
    }

    /// The current pre-edit, if composing.
    pub fn preedit(&self) -> Option<&Preedit> {
        self.preedit.as_ref()
    }

    /// The buffer as displayed: committed text with the preedit spliced in at
    /// the cursor.
    pub fn display_text(&self) -> String {
        match &self.preedit {
            Some(p) => format!(
                "{}{}{}",
                &self.text[..self.cursor],
                p.text,
                &self.text[self.cursor..]
            ),
            None => self.text.clone(),
        }
    }

    // --- editing ------------------------------------------------------------

    fn snapshot(&mut self) {
        self.undo.push(Snapshot {
            text: self.text.clone(),
            cursor: self.cursor,
            anchor: self.anchor,
        });
        self.redo.clear();
    }

    fn replace_selection(&mut self, s: &str) {
        let (a, b) = self.selection();
        self.text.replace_range(a..b, s);
        self.cursor = a + s.len();
        self.anchor = self.cursor;
    }

    /// Insert `s`, replacing any selection.
    pub fn insert(&mut self, s: &str) {
        self.snapshot();
        self.replace_selection(s);
    }

    /// Delete the selection, or the char before the cursor.
    pub fn backspace(&mut self) {
        self.snapshot();
        if self.has_selection() {
            self.replace_selection("");
        } else if self.cursor > 0 {
            let prev = self.prev_boundary(self.cursor);
            self.text.replace_range(prev..self.cursor, "");
            self.cursor = prev;
            self.anchor = prev;
        } else {
            self.undo.pop(); // nothing changed
        }
    }

    /// Delete the selection, or the char after the cursor.
    pub fn delete(&mut self) {
        self.snapshot();
        if self.has_selection() {
            self.replace_selection("");
        } else if self.cursor < self.text.len() {
            let next = self.next_boundary(self.cursor);
            self.text.replace_range(self.cursor..next, "");
        } else {
            self.undo.pop();
        }
    }

    // --- navigation ---------------------------------------------------------

    /// Move the cursor left one char (`extend` keeps the selection anchor).
    pub fn move_left(&mut self, extend: bool) {
        self.cursor = self.prev_boundary(self.cursor);
        if !extend {
            self.anchor = self.cursor;
        }
    }
    /// Move the cursor right one char.
    pub fn move_right(&mut self, extend: bool) {
        self.cursor = self.next_boundary(self.cursor);
        if !extend {
            self.anchor = self.cursor;
        }
    }
    /// Move to the start of the buffer.
    pub fn move_home(&mut self, extend: bool) {
        self.cursor = 0;
        if !extend {
            self.anchor = 0;
        }
    }
    /// Move to the end of the buffer.
    pub fn move_end(&mut self, extend: bool) {
        self.cursor = self.text.len();
        if !extend {
            self.anchor = self.cursor;
        }
    }
    /// Select the whole buffer.
    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
    }
    /// Set the selection explicitly.
    pub fn set_selection(&mut self, anchor: usize, cursor: usize) {
        self.anchor = anchor.min(self.text.len());
        self.cursor = cursor.min(self.text.len());
    }

    /// Place the cursor at byte offset `byte` (clamped to a char boundary). With
    /// `extend` false the selection collapses there (a plain click / caret move);
    /// with `extend` true the anchor is kept (drag-select / Shift-click).
    pub fn place(&mut self, byte: usize, extend: bool) {
        let mut b = byte.min(self.text.len());
        while b > 0 && !self.text.is_char_boundary(b) {
            b -= 1;
        }
        self.cursor = b;
        if !extend {
            self.anchor = b;
        }
    }

    // --- clipboard ----------------------------------------------------------

    /// Copy the selection.
    pub fn copy(&self) -> String {
        self.selected_text()
    }
    /// Cut the selection (returns it).
    pub fn cut(&mut self) -> String {
        let s = self.selected_text();
        if !s.is_empty() {
            self.insert("");
        }
        s
    }
    /// Paste `s` at the cursor (replacing any selection).
    pub fn paste(&mut self, s: &str) {
        self.insert(s);
    }

    // --- IME ----------------------------------------------------------------

    /// Set the pre-edit (composition) text.
    pub fn set_preedit(&mut self, text: &str, cursor: Option<(usize, usize)>) {
        self.preedit = Some(Preedit {
            text: text.to_string(),
            cursor,
        });
    }
    /// Clear the pre-edit without committing.
    pub fn clear_preedit(&mut self) {
        self.preedit = None;
    }
    /// Commit `text` (post-IME) and clear the pre-edit.
    pub fn commit(&mut self, text: &str) {
        self.preedit = None;
        self.insert(text);
    }

    // --- history ------------------------------------------------------------

    /// Undo the last edit.
    pub fn undo(&mut self) {
        if let Some(snap) = self.undo.pop() {
            self.redo.push(Snapshot {
                text: self.text.clone(),
                cursor: self.cursor,
                anchor: self.anchor,
            });
            self.text = snap.text;
            self.cursor = snap.cursor;
            self.anchor = snap.anchor;
        }
    }
    /// Redo the last undone edit.
    pub fn redo(&mut self) {
        if let Some(snap) = self.redo.pop() {
            self.undo.push(Snapshot {
                text: self.text.clone(),
                cursor: self.cursor,
                anchor: self.anchor,
            });
            self.text = snap.text;
            self.cursor = snap.cursor;
            self.anchor = snap.anchor;
        }
    }

    fn prev_boundary(&self, i: usize) -> usize {
        if i == 0 {
            return 0;
        }
        let mut j = i - 1;
        while !self.text.is_char_boundary(j) {
            j -= 1;
        }
        j
    }
    fn next_boundary(&self, i: usize) -> usize {
        let mut j = (i + 1).min(self.text.len());
        while j < self.text.len() && !self.text.is_char_boundary(j) {
            j += 1;
        }
        j
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_select_delete() {
        let mut e = TextEditor::new("hello");
        e.move_home(false);
        e.move_right(true);
        e.move_right(true); // select "he"
        assert_eq!(e.selected_text(), "he");
        e.insert("HE");
        assert_eq!(e.text(), "HEllo");
        e.move_end(false);
        e.backspace();
        assert_eq!(e.text(), "HEll");
    }

    #[test]
    fn undo_redo() {
        let mut e = TextEditor::new("");
        e.insert("a");
        e.insert("b");
        assert_eq!(e.text(), "ab");
        e.undo();
        assert_eq!(e.text(), "a");
        e.undo();
        assert_eq!(e.text(), "");
        e.redo();
        assert_eq!(e.text(), "a");
    }

    #[test]
    fn clipboard() {
        let mut e = TextEditor::new("abcdef");
        e.set_selection(1, 4); // "bcd"
        let cut = e.cut();
        assert_eq!(cut, "bcd");
        assert_eq!(e.text(), "aef");
        e.paste("X");
        assert_eq!(e.text(), "aXef");
    }

    #[test]
    fn serde_round_trip_preserves_state_and_history() {
        let mut e = TextEditor::new("hello");
        e.move_home(false);
        e.move_right(true);
        e.move_right(true); // select "he"
        e.insert("HE"); // pushes an undo snapshot
                        // Round-trip through JSON (the Signal<T> storage format).
        let json = serde_json::to_string(&e).unwrap();
        let mut back: TextEditor = serde_json::from_str(&json).unwrap();
        assert_eq!(back.text(), "HEllo");
        assert_eq!(back.cursor(), e.cursor());
        assert_eq!(back.selection(), e.selection());
        // Undo history survived the round-trip.
        back.undo();
        assert_eq!(back.text(), "hello");
    }

    #[test]
    fn ime_cjk_composition() {
        let mut e = TextEditor::new("");
        // type "ni" then "nihao" preedit, then commit 你好
        e.set_preedit("ni", Some((2, 2)));
        assert_eq!(e.display_text(), "ni");
        assert_eq!(e.text(), "", "preedit is not committed");
        e.set_preedit("nihao", Some((5, 5)));
        assert_eq!(e.display_text(), "nihao");
        e.commit("你好");
        assert_eq!(e.text(), "你好");
        assert!(e.preedit().is_none());
        // a following preedit composes after 你好
        e.set_preedit("shijie", None);
        assert_eq!(e.display_text(), "你好shijie");
        e.commit("世界");
        assert_eq!(e.text(), "你好世界");
    }
}
