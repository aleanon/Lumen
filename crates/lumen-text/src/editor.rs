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

/// Maximum retained undo steps. Each step holds a full copy of the document,
/// so this is the knob that turns unbounded growth into a fixed ceiling.
const UNDO_LIMIT: usize = 256;

/// How many of the oldest steps to drop when [`UNDO_LIMIT`] is exceeded.
/// Batching keeps the amortized cost of trimming near one element-move per
/// edit instead of a full shift every keystroke past the cap.
const UNDO_TRIM: usize = 64;

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
        // CACHE1: bound the history.
        //
        // Each snapshot clones the WHOLE document, and the stack was
        // unbounded, so typing N characters into a document of length L cost
        // O(N*L) memory — the only quadratic growth in the codebase, and it
        // grows fastest exactly where it hurts most: a long document being
        // edited for a long time.
        //
        // Capping the depth is the small fix; the principled one is delta-based
        // history (store the edit, not the document), which is a different
        // project. A cap is not a compromise on correctness — every editor has
        // a finite undo depth — it just makes the bound explicit instead of
        // "until memory runs out".
        //
        // Trimmed in a batch rather than one-at-a-time so the O(n) shift is
        // amortized to roughly one element-move per edit.
        if self.undo.len() > UNDO_LIMIT {
            self.undo.drain(..UNDO_TRIM);
        }
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
    // --- word and line granularity -----------------------------------------
    //
    // A "word" here is the convention every desktop editor shares: moving left
    // skips any whitespace immediately behind the cursor, then the run of word
    // characters behind that; moving right skips the run of word characters
    // ahead, then the whitespace after it. That asymmetry is what makes
    // Ctrl+Left/Ctrl+Right land on word *starts* going both ways.

    /// The byte offset one word to the left of `at`.
    pub fn prev_word(&self, at: usize) -> usize {
        let b = self.text.as_bytes();
        let mut i = at;
        while i > 0 && !is_word_byte(b, self.prev_boundary(i)) {
            i = self.prev_boundary(i);
        }
        while i > 0 && is_word_byte(b, self.prev_boundary(i)) {
            i = self.prev_boundary(i);
        }
        i
    }

    /// The byte offset one word to the right of `at`.
    pub fn next_word(&self, at: usize) -> usize {
        let b = self.text.as_bytes();
        let mut i = at;
        while i < self.text.len() && is_word_byte(b, i) {
            i = self.next_boundary(i);
        }
        while i < self.text.len() && !is_word_byte(b, i) {
            i = self.next_boundary(i);
        }
        i
    }

    /// Move the cursor one word left (Ctrl+Left).
    pub fn move_word_left(&mut self, extend: bool) {
        self.cursor = self.prev_word(self.cursor);
        if !extend {
            self.anchor = self.cursor;
        }
    }

    /// Move the cursor one word right (Ctrl+Right).
    pub fn move_word_right(&mut self, extend: bool) {
        self.cursor = self.next_word(self.cursor);
        if !extend {
            self.anchor = self.cursor;
        }
    }

    /// Delete the selection, or the word before the cursor (Ctrl+Backspace).
    pub fn delete_word_left(&mut self) {
        if self.has_selection() {
            self.backspace();
            return;
        }
        let from = self.prev_word(self.cursor);
        if from == self.cursor {
            return;
        }
        self.snapshot();
        self.text.replace_range(from..self.cursor, "");
        self.cursor = from;
        self.anchor = from;
    }

    /// Delete the selection, or the word after the cursor (Ctrl+Delete).
    pub fn delete_word_right(&mut self) {
        if self.has_selection() {
            self.delete();
            return;
        }
        let to = self.next_word(self.cursor);
        if to == self.cursor {
            return;
        }
        self.snapshot();
        self.text.replace_range(self.cursor..to, "");
    }

    /// The `(start, end)` byte range of the line containing `at`, excluding the
    /// trailing newline.
    pub fn line_bounds(&self, at: usize) -> (usize, usize) {
        let at = at.min(self.text.len());
        let start = self.text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let end = self.text[at..]
            .find('\n')
            .map(|i| at + i)
            .unwrap_or(self.text.len());
        (start, end)
    }

    /// Move to the start of the current line (Home in a multi-line field).
    pub fn move_line_home(&mut self, extend: bool) {
        self.cursor = self.line_bounds(self.cursor).0;
        if !extend {
            self.anchor = self.cursor;
        }
    }

    /// Move to the end of the current line (End in a multi-line field).
    pub fn move_line_end(&mut self, extend: bool) {
        self.cursor = self.line_bounds(self.cursor).1;
        if !extend {
            self.anchor = self.cursor;
        }
    }

    /// Select the whole line the cursor is on (Ctrl+L).
    pub fn select_line(&mut self) {
        let (a, b) = self.line_bounds(self.cursor);
        self.anchor = a;
        self.cursor = b;
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

    /// Number of retained undo steps. Bounded by `UNDO_LIMIT` (CACHE1).
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

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

/// Whether the byte at `i` starts a word character.
///
/// ASCII alphanumerics and `_` are word bytes; so is every non-ASCII byte,
/// which keeps accented and CJK text inside a word instead of treating each
/// multi-byte char as its own token.
fn is_word_byte(b: &[u8], i: usize) -> bool {
    match b.get(i) {
        Some(c) => c.is_ascii_alphanumeric() || *c == b'_' || !c.is_ascii(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_motion_lands_on_word_starts() {
        let mut e = TextEditor::new("the quick  brown fox");
        e.move_word_left(false);
        assert_eq!(e.cursor(), 17, "from the end, back to the start of `fox`");
        e.move_word_left(false);
        assert_eq!(e.cursor(), 11, "past the double space to `brown`");
        e.move_home(false);
        e.move_word_right(false);
        assert_eq!(e.cursor(), 4, "forward stops at the next word start");
    }

    #[test]
    fn ctrl_backspace_eats_one_word_and_its_gap() {
        let mut e = TextEditor::new("hello brave world");
        e.delete_word_left();
        assert_eq!(e.text(), "hello brave ");
        e.delete_word_left();
        assert_eq!(e.text(), "hello ");
        // Undo restores in the same steps, so a mis-hit is recoverable.
        e.undo();
        assert_eq!(e.text(), "hello brave ");
    }

    #[test]
    fn ctrl_delete_eats_forward() {
        let mut e = TextEditor::new("hello brave world");
        e.move_home(false);
        e.delete_word_right();
        assert_eq!(e.text(), "brave world");
    }

    #[test]
    fn word_ops_on_a_selection_act_on_the_selection() {
        let mut e = TextEditor::new("hello world");
        e.select_all();
        e.delete_word_left();
        assert_eq!(e.text(), "", "a selection is what gets deleted, not a word");
    }

    #[test]
    fn line_ops_are_line_relative() {
        let mut e = TextEditor::new("first line\nsecond line\nthird");
        e.place(14, false); // inside "second"
        assert_eq!(e.line_bounds(14), (11, 22));
        e.move_line_home(false);
        assert_eq!(e.cursor(), 11);
        e.move_line_end(false);
        assert_eq!(e.cursor(), 22);
        e.select_line();
        assert_eq!(e.selected_text(), "second line");
    }

    #[test]
    fn select_line_on_a_single_line_buffer_takes_all_of_it() {
        let mut e = TextEditor::new("just one line");
        e.select_line();
        assert_eq!(e.selected_text(), "just one line");
    }

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

#[cfg(test)]
mod history_bound_tests {
    use super::*;

    #[test]
    fn undo_history_is_bounded() {
        // Before CACHE1 this grew without limit, and each entry holds a full
        // copy of the document — so a long editing session on a large document
        // grew memory quadratically.
        let mut ed = TextEditor::new("");
        for i in 0..(UNDO_LIMIT * 3) {
            ed.insert(&format!("{i} "));
        }
        assert!(
            ed.undo_depth() <= UNDO_LIMIT,
            "undo history must stay bounded, got {}",
            ed.undo_depth()
        );
    }

    #[test]
    fn recent_history_still_undoes_correctly() {
        // Capping must drop the OLDEST steps, never disturb recent ones —
        // a bound that broke undo would be worse than the leak.
        let mut ed = TextEditor::new("");
        for _ in 0..(UNDO_LIMIT + 10) {
            ed.insert("x");
        }
        let before = ed.text().to_string();
        ed.undo();
        assert_eq!(
            ed.text().len(),
            before.len() - 1,
            "the most recent edit must still be undoable after trimming"
        );
    }
}
