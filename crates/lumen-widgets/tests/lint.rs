//! Item 4: the absolute visual-invariant lint (overflow / clipping / zero-area
//! interactive). Unlike goldens it catches *first-time* defects — a clean UI has
//! no findings; a broken one is flagged.

use kurbo::Size;
use lumen_core::codes;
use lumen_widgets::{App, Button, Element, Label};

#[test]
fn a_normal_ui_has_no_lint_findings() {
    let h = App::new(|_| {
        Element::column(vec![
            Label::new("Hello, gypq — descenders and Ástërisks").into(),
            Element::row(vec![Button::new("OK").into(), Button::new("Cancel").into()]),
            Label::new("A second paragraph of body text.").into(),
        ])
    })
    .run_headless(Size::new(400.0, 300.0));
    let findings = h.lint();
    assert!(
        findings.is_empty(),
        "a normal UI must be lint-clean; got {findings:?}"
    );
}

#[test]
fn clipped_text_is_caught_by_lint() {
    let h = App::new(|_| Label::new("gypq jQ").line_height(1.0).into())
        .run_headless(Size::new(240.0, 80.0));
    assert!(
        h.lint().iter().any(|d| d.code == codes::W0104),
        "lint must catch the clipped text; got {:?}",
        h.lint()
    );
}
