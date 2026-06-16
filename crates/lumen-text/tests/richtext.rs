//! T6.5: rich-text doc model, find/replace, and cross-widget selection.
use lumen_text::richtext::{selected_text, CrossSelection, RichDoc};

#[test]
fn styles_and_find_replace() {
    let mut doc = RichDoc::new("hello world hello");
    doc.apply_style(0, 5, true, false); // bold "hello"
    assert!(doc.is_bold_at(2));
    assert!(!doc.is_bold_at(8));

    // Find all "hello".
    assert_eq!(doc.find("hello"), vec![(0, 5), (12, 17)]);
    // Replace.
    assert_eq!(doc.replace_all("hello", "hi"), 2);
    assert_eq!(doc.text(), "hi world hi");
    assert!(doc.runs().is_empty(), "runs reset after structural edit");
}

#[test]
fn insert_and_overlapping_finds() {
    let mut doc = RichDoc::new("aaaa");
    // Non-overlapping matches of "aa".
    assert_eq!(doc.find("aa"), vec![(0, 2), (2, 4)]);
    doc.insert(2, "X");
    assert_eq!(doc.text(), "aaXaa");
}

#[test]
fn cross_widget_selection() {
    let widgets = ["Hello", "brave", "world"];
    // From the middle of widget 0 to the middle of widget 2.
    let sel = CrossSelection {
        start: (0, 2),
        end: (2, 3),
    };
    assert_eq!(selected_text(&widgets, sel), "llo\nbrave\nwor");
    // Reversed anchor/focus selects the same text.
    let rev = CrossSelection {
        start: (2, 3),
        end: (0, 2),
    };
    assert_eq!(selected_text(&widgets, rev), "llo\nbrave\nwor");
    // Within a single widget.
    let one = CrossSelection {
        start: (1, 0),
        end: (1, 5),
    };
    assert_eq!(selected_text(&widgets, one), "brave");
}
