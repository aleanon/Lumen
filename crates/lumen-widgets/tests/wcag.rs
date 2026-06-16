//! T7.4: WCAG contrast ratios + accessible-name audit.
use kurbo::Size;
use lumen_core::Color;
use lumen_widgets::wcag::{audit_names, contrast_ratio, meets_aa};
use lumen_widgets::{widgets, App, BuildCx};

#[test]
fn contrast_ratios() {
    let black = Color::srgb8(0, 0, 0, 255);
    let white = Color::srgb8(255, 255, 255, 255);
    assert!(
        (contrast_ratio(black, white) - 21.0).abs() < 0.1,
        "black/white = 21:1"
    );
    assert!(
        (contrast_ratio(white, white) - 1.0).abs() < 1e-6,
        "same color = 1:1"
    );
    // White text on the brand blue passes AA; mid-grey on white fails.
    assert!(meets_aa(white, Color::srgb8(0x1a, 0x73, 0xe8, 0xff), false));
    assert!(!meets_aa(
        Color::srgb8(0x99, 0x99, 0x99, 0xff),
        white,
        false
    ));
}

#[test]
fn accessible_name_audit() {
    // A labelled button passes; an unnamed interactive node is flagged.
    let named = App::new(|_: &mut BuildCx| widgets::button("Save", |_| {}).id("ok"))
        .run_headless(Size::new(120.0, 60.0));
    assert!(audit_names(&named.semantics_doc().root.elided()).is_empty());

    let unnamed = App::new(|_: &mut BuildCx| widgets::button("", |_| {}).id("bad"))
        .run_headless(Size::new(120.0, 60.0));
    let issues = audit_names(&unnamed.semantics_doc().root.elided());
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].id.as_deref(), Some("bad"));
}
