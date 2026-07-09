//! B.7a (docs/plan-remediation-2026-07.md): value type mismatches on the
//! applied property set emit `E0103` with the expected type and reject the
//! sheet atomically (04 §9 — the code was defined-but-dead; `apply()`
//! silently ignored bad values). `border-width`/`border-color` no longer
//! raise a spurious `E0102`.

use lumen_style::{has_errors, parse};

#[test]
fn type_mismatches_emit_e0103_and_reject() {
    for bad in [
        "#x { opacity: red; }",
        "#x { background: 12px; }",
        "#x { width: #ff0000ff; }",
        "#x { display: 4px; }",
        "#x { font-weight: bold-ish; }",
    ] {
        let (_, diags) = parse("t.lss", bad);
        assert!(
            diags.iter().any(|d| d.code == "E0103"),
            "{bad} should E0103: {diags:?}"
        );
        assert!(has_errors(&diags), "{bad} must reject the sheet");
    }
}

#[test]
fn valid_indirect_and_shorthand_values_pass() {
    for good in [
        "#x { opacity: 0.5; }",
        "#x { width: 120px; height: auto; }",
        "@tokens { primary: #112233ff; }\n#x { background: $primary; }",
        "#x { border-width: 2px; border-color: #112233ff; }",
        "#x { display: flex; flex-direction: row; }",
        "#x { backdrop-filter: blur(8px) saturate(1.2); }",
    ] {
        let (_, diags) = parse("t.lss", good);
        assert!(
            !diags.iter().any(|d| d.code == "E0103" || d.code == "E0102"),
            "{good} should be clean: {diags:?}"
        );
    }
}
