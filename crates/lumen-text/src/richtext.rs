//! Rich-text document model (T6.5): styled runs over a text buffer, find/replace,
//! and a cross-widget selection model. The verifiable core of advanced editing;
//! lists/tables/links/images, spell-check, variable-font axes, and CRDT hooks
//! layer on this `RichDoc`.

/// A styled run over `[start, end)` byte offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleRun {
    /// Start byte offset.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
}

/// A rich-text document: a text buffer plus style runs.
#[derive(Clone, Debug, Default)]
pub struct RichDoc {
    text: String,
    runs: Vec<StyleRun>,
}

impl RichDoc {
    /// A document from plain text.
    pub fn new(text: impl Into<String>) -> RichDoc {
        RichDoc {
            text: text.into(),
            runs: Vec::new(),
        }
    }

    /// The plain text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The style runs.
    pub fn runs(&self) -> &[StyleRun] {
        &self.runs
    }

    /// Apply bold/italic over a byte range.
    pub fn apply_style(&mut self, start: usize, end: usize, bold: bool, italic: bool) {
        if start < end && end <= self.text.len() {
            self.runs.push(StyleRun {
                start,
                end,
                bold,
                italic,
            });
        }
    }

    /// Whether the byte at `offset` is bold under any run.
    pub fn is_bold_at(&self, offset: usize) -> bool {
        self.runs
            .iter()
            .any(|r| r.bold && (r.start..r.end).contains(&offset))
    }

    /// Byte ranges of every (non-overlapping, left-to-right) match of `query`.
    pub fn find(&self, query: &str) -> Vec<(usize, usize)> {
        if query.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(i) = self.text[from..].find(query) {
            let start = from + i;
            out.push((start, start + query.len()));
            from = start + query.len();
        }
        out
    }

    /// Replace every match of `query` with `with`; returns the count. Style runs
    /// are dropped (re-styling after a structural edit is the caller's job).
    pub fn replace_all(&mut self, query: &str, with: &str) -> usize {
        let n = self.find(query).len();
        if n > 0 {
            self.text = self.text.replace(query, with);
            self.runs.clear();
        }
        n
    }

    /// Insert `s` at byte `offset`.
    pub fn insert(&mut self, offset: usize, s: &str) {
        if offset <= self.text.len() {
            self.text.insert_str(offset, s);
        }
    }
}

/// A selection that spans several text widgets, by `(widget_index, byte_offset)`.
#[derive(Clone, Copy, Debug)]
pub struct CrossSelection {
    /// Anchor `(widget, offset)`.
    pub start: (usize, usize),
    /// Focus `(widget, offset)`.
    pub end: (usize, usize),
}

/// The text selected across `widgets` (joined with `\n` at widget boundaries).
pub fn selected_text(widgets: &[&str], sel: CrossSelection) -> String {
    let (mut a, mut b) = (sel.start, sel.end);
    if a > b {
        std::mem::swap(&mut a, &mut b);
    }
    let mut out = String::new();
    for (i, w) in widgets.iter().enumerate() {
        if i < a.0 || i > b.0 {
            continue;
        }
        let from = if i == a.0 { a.1.min(w.len()) } else { 0 };
        let to = if i == b.0 { b.1.min(w.len()) } else { w.len() };
        if i > a.0 {
            out.push('\n');
        }
        out.push_str(&w[from..to]);
    }
    out
}
