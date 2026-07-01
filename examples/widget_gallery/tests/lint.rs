//! Item 4 CI invariant: the whole widget gallery is visual-lint-clean (no
//! overflow / clipping / zero-area interactive nodes). Catches first-time layout
//! or render defects that goldens (regression-only) would enshrine.

use kurbo::Size;

#[test]
fn gallery_is_lint_clean() {
    let h = widget_gallery::main_app().run_headless(Size::new(620.0, 980.0));
    let findings = h.lint();
    assert!(
        findings.is_empty(),
        "widget gallery must have no visual-invariant findings; got {findings:?}"
    );
}
