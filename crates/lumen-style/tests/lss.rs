//! T1.1 acceptance: parser corpus (valid + ≥30 error fixtures asserting
//! codes/spans/did-you-mean) and cascade/specificity table tests.

use lumen_style::{parse, resolve, NodeDesc, Origin, Specificity, StyleSource};

fn diags(src: &str) -> Vec<lumen_core::Diagnostic> {
    parse("test.lss", src).1
}
fn codes(src: &str) -> Vec<&'static str> {
    diags(src).iter().map(|d| d.code).collect()
}

const VALID: &str = r#"
// design tokens
@tokens {
    spacing-1: 4px;
    radius: 6px;
    font-ui: "Inter", "Noto Sans";
}
@theme light { primary: oklch(0.62 0.19 255); bg: #ffffff; border: #d8dde3; }
@theme dark  { primary: oklch(0.72 0.17 255); bg: #101418; }

button.primary {
    background: $primary;
    color: $bg;
    border-radius: $radius;
    padding: 8px;
    transition: background 120ms ease;
    &:hover { background: $primary; }
    &:disabled { opacity: 0.45; }
}

dialog .footer > button {
    margin-left: $spacing-1;
}

@keyframes spin {
    0% { transform: rotate(0deg); }
    100% { transform: rotate(360deg); }
}

@media (width >= 600px) {
    button { padding: 12px; }
}
"#;

#[test]
fn valid_corpus_parses_cleanly() {
    let (sheet, ds) = parse("ok.lss", VALID);
    assert!(ds.is_empty(), "expected no diagnostics, got: {ds:?}");
    assert!(!sheet.items.is_empty());
}

#[test]
fn error_fixtures_report_expected_codes() {
    // (source, expected diagnostic code)
    let fixtures: &[(&str, &str)] = &[
        // --- E0101 syntax (15) ---
        ("button { color: }", "E0101"),
        ("button { color red; }", "E0101"),
        ("button color: red; }", "E0101"),
        ("{ color: red; }", "E0101"),
        ("@unknown { }", "E0101"),
        ("button { color: #zzz; }", "E0101"),
        ("@theme purple { }", "E0101"),
        ("button {", "E0101"),
        ("@media width: 800px) { }", "E0101"),
        ("@media (foo) { }", "E0101"),
        ("button { color:: red; }", "E0101"),
        ("button { 123: red; }", "E0101"),
        ("@keyframes { }", "E0101"),
        ("\"str\" { color: red; }", "E0101"),
        ("button { ; }", "E0101"),
        // --- E0102 unknown property + did-you-mean (8) ---
        ("button { colr: red; }", "E0102"),
        ("button { widht: 10px; }", "E0102"),
        ("button { paddng: 5px; }", "E0102"),
        ("button { colour: red; }", "E0102"),
        ("button { font-sze: 12px; }", "E0102"),
        ("button { z-idex: 1; }", "E0102"),
        ("button { hightnotreal: 1px; }", "E0102"),
        ("button { bckground: red; }", "E0102"),
        // --- E0104 unknown token + did-you-mean (7) ---
        ("button { color: $undefined; }", "E0104"),
        ("@tokens { primary: #fff; } b { color: $primry; }", "E0104"),
        ("button { background: $bg; }", "E0104"),
        (
            "@tokens { border: #ccc; } b { border: $bordr 1px; }",
            "E0104",
        ),
        ("@theme light { ab: #000; } .x { color: $a; }", "E0104"),
        (
            "@tokens { spacing-1: 4px; } b { gap: $spacing-2; }",
            "E0104",
        ),
        (
            "@tokens { radius: 6px; } b { border-radius: $radus; }",
            "E0104",
        ),
    ];
    assert!(fixtures.len() >= 30, "need >= 30 error fixtures");
    for (src, code) in fixtures {
        let got = codes(src);
        assert!(
            got.contains(code),
            "source {src:?}: expected {code}, got {got:?}"
        );
    }
}

#[test]
fn did_you_mean_and_span() {
    // E0102 suggests the nearest property.
    let d = &diags("button { colr: red; }")[0];
    assert_eq!(d.code, "E0102");
    assert!(d.message.contains("color"), "msg: {}", d.message);
    assert!(d.span.is_some());

    // E0104 suggests the nearest defined token.
    let ds = diags("@tokens { primary: #fff; } b { color: $primry; }");
    let e = ds.iter().find(|d| d.code == "E0104").unwrap();
    assert!(e.message.contains("primary"), "msg: {}", e.message);

    // Span points at the offending line.
    let ds = diags("button {\n  colr: red;\n}");
    let e = ds.iter().find(|d| d.code == "E0102").unwrap();
    assert_eq!(e.span.as_ref().unwrap().line, 2);
}

fn spec(selector: &str) -> Specificity {
    let src = format!("{selector} {{ width: 1px; }}");
    let (sheet, ds) = parse("s.lss", &src);
    assert!(
        ds.is_empty(),
        "selector {selector:?} had diagnostics: {ds:?}"
    );
    match &sheet.items[0] {
        lumen_style::Item::Rule(r) => r.selectors[0].specificity(),
        _ => unreachable!(),
    }
}

#[test]
fn specificity_table() {
    let s = |id, class, ty| Specificity { id, class, ty };
    assert_eq!(spec("button"), s(0, 0, 1));
    assert_eq!(spec(".primary"), s(0, 1, 0));
    assert_eq!(spec("#save"), s(1, 0, 0));
    assert_eq!(spec("button.primary"), s(0, 1, 1));
    assert_eq!(spec("button.primary:hover"), s(0, 2, 1));
    assert_eq!(spec("dialog .footer button"), s(0, 1, 2));
    assert_eq!(spec("#a.b.c"), s(1, 2, 0));
    // specificity ordering
    assert!(spec("#save") > spec("button.primary"));
    assert!(spec("button.primary") > spec("button"));
}

fn app(src: &str) -> StyleSource {
    StyleSource {
        origin: Origin::App,
        sheet: parse("a.lss", src).0,
    }
}

#[test]
fn cascade_table() {
    let node = NodeDesc {
        id: Some("save".into()),
        classes: vec!["primary".into()],
        states: vec![],
        ty: "button".into(),
    };

    // Higher specificity wins.
    let sources = [app(
        "button { color: #ff0000ff; } button.primary { color: #0000ffff; }",
    )];
    let r = resolve(&sources, &node);
    assert_eq!(
        r["color"].value,
        lumen_style::Value::Color(lumen_core::Color::from_hex("#0000ffff").unwrap())
    );

    // !important beats higher specificity.
    let sources = [app(
        "button { color: #00ff00ff !important; } button.primary { color: #0000ffff; }",
    )];
    let r = resolve(&sources, &node);
    assert_eq!(
        r["color"].value,
        lumen_style::Value::Color(lumen_core::Color::from_hex("#00ff00ff").unwrap())
    );
    assert!(r["color"].important);

    // Later origin wins over earlier at equal specificity.
    let sources = [
        StyleSource {
            origin: Origin::Default,
            sheet: parse("d.lss", "button { color: #ff0000ff; }").0,
        },
        app("button { color: #0000ffff; }"),
    ];
    let r = resolve(&sources, &node);
    assert_eq!(r["color"].origin, Origin::App);
}
