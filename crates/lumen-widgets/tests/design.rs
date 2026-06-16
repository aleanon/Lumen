//! T7.5: a design spec maps to valid .lss (round-trips through the parser).
use lumen_widgets::design::spec_to_lss;
use serde_json::json;

#[test]
fn spec_imports_to_valid_lss() {
    let spec = json!({
        "tokens": { "accent": "#1a73e8ff" },
        "rules": { "#title": { "color": "$accent" } }
    });
    let lss = spec_to_lss(&spec);
    assert!(lss.contains("@tokens {") && lss.contains("accent: #1a73e8ff;"));
    assert!(lss.contains("#title {") && lss.contains("color: $accent;"));

    // The generated .lss parses with no errors.
    let (_sheet, diags) = lumen_style::parse("design.lss", &lss);
    assert!(
        !lumen_style::has_errors(&diags),
        "imported lss is valid: {diags:?}"
    );
}
