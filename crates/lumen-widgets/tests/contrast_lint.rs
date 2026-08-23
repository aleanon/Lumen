//! O1.1: `ui.lint` reports text that cannot be read (W0303).
//!
//! `contrast_report()` — APCA measured against the *composited* backdrop — was
//! fully implemented, tested, and had no caller on the lint path. So the defect
//! a human spots instantly (white text on a white card) was one the agent could
//! not observe at all, while `.ai_docs/03 §ui.lint` claimed contrast coverage.
//!
//! These tests pin the *floor*, not the grading. `ContrastLevel` keeps the
//! graded APCA tiers for callers assessing a palette; W0303 fires only when
//! text is invisible, which is a defect under any design.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};

fn w0303(lss: &str) -> Vec<String> {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![widgets::text("Invisible label").id("a")]).id("root")
    })
    .run_headless(Size::new(200.0, 100.0));
    h.set_stylesheet(lss);
    h.pump();
    h.lint()
        .into_iter()
        .filter(|d| d.code == "W0303")
        .map(|d| d.message)
        .collect()
}

#[test]
fn white_on_white_text_is_reported() {
    let found = w0303("#root { background: #ffffff; } #a { color: #ffffff; }");
    assert_eq!(
        found.len(),
        1,
        "white-on-white must be reported exactly once: {found:?}"
    );
    assert!(
        found[0].contains("unreadable"),
        "the message must say what is wrong: {}",
        found[0]
    );
}

#[test]
fn readable_text_is_not_reported() {
    let found = w0303("#root { background: #ffffff; } #a { color: #000000; }");
    assert!(
        found.is_empty(),
        "black on white is maximally readable; must not fire: {found:?}"
    );
}

#[test]
fn merely_low_contrast_text_is_not_reported() {
    // Mid-grey on white is poor design and entirely legitimate output.
    // `ContrastLevel::Fail` begins at |Lc| 45 and would fire here; the W0303
    // floor is a legibility line, not a palette opinion, and must not.
    let found = w0303("#root { background: #ffffff; } #a { color: #949494; }");
    assert!(
        found.is_empty(),
        "low-but-readable contrast must not raise a hard diagnostic: {found:?}"
    );
}
