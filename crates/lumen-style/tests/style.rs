//! T1.2 acceptance: `.lss`↔typed `Style` parity, computed-value serialization
//! (04 §7), and light/dark theme-token resolution.

use lumen_layout::Display;
use lumen_style::{apply, canonical, computed_json, tokens_for, Origin, Style, ThemeKind, Tokens};
use lumen_style::{Item, Value};

/// Parse a single `prop: val;` declaration's value.
fn val(prop: &str, v: &str) -> Value {
    let src = format!("x {{ {prop}: {v}; }}");
    let (sheet, ds) = lumen_style::parse("t.lss", &src);
    assert!(ds.is_empty(), "{prop}: {v} -> {ds:?}");
    match &sheet.items[0] {
        Item::Rule(r) => r.declarations[0].value.clone(),
        _ => unreachable!(),
    }
}

/// Assert the `.lss` value and the typed setter produce the same `Style`.
macro_rules! style_parity {
    ($prop:literal, $lss:literal, $typed:expr) => {{
        let mut from_lss = Style::new();
        apply(&mut from_lss, $prop, &val($prop, $lss), &Tokens::new());
        let from_typed: Style = $typed(Style::new());
        assert_eq!(from_lss, from_typed, "parity for {}: {}", $prop, $lss);
    }};
}

#[test]
fn lss_matches_typed_mirror() {
    use lumen_core::Color;
    style_parity!("background", "#1a73e8ff", |s: Style| s
        .background(Color::from_hex("#1a73e8ff").unwrap()));
    style_parity!("color", "#ffffffff", |s: Style| s
        .color(Color::from_hex("#ffffffff").unwrap()));
    style_parity!("padding", "8px", |s: Style| s.padding(8.0));
    style_parity!("border-radius", "6px", |s: Style| s.radius(6.0));
    style_parity!("opacity", "0.45", |s: Style| s.opacity(0.45));
    style_parity!("font-size", "16px", |s: Style| s.font_size(16.0));
    style_parity!("font-weight", "600", |s: Style| s.font_weight(600));
    style_parity!("width", "100px", |s: Style| s.width(100.0));
    style_parity!("gap", "8px", |s: Style| s.gap(8.0));
    style_parity!("display", "flex", |s: Style| s.display(Display::Flex));
}

#[test]
fn computed_value_serialization() {
    assert_eq!(
        canonical(&val("width", "8px")),
        serde_json::json!({ "px": 8.0 })
    );
    assert_eq!(
        canonical(&val("background", "#1a73e8ff")),
        serde_json::json!("#1a73e8ff")
    );
    assert_eq!(
        canonical(&val("display", "flex")),
        serde_json::json!("flex")
    );
    // s normalizes to ms
    assert_eq!(
        canonical(&val("transition", "120ms")),
        serde_json::json!({ "ms": 120.0 })
    );

    let c = computed_json(&val("opacity", "0.5"), Origin::App);
    assert_eq!(c["value"], serde_json::json!({ "px": 0.5 }));
    assert_eq!(c["source"], "stylesheet");
}

#[test]
fn light_and_dark_themes_resolve_differently() {
    let src = r#"
        @theme light { primary: oklch(0.62 0.19 255); }
        @theme dark  { primary: oklch(0.72 0.17 255); }
        button { background: $primary; }
    "#;
    let (sheet, ds) = lumen_style::parse("t.lss", src);
    assert!(ds.is_empty(), "{ds:?}");
    let bg = match &sheet.items[2] {
        Item::Rule(r) => r.declarations[0].value.clone(),
        _ => unreachable!(),
    };

    let mut light = Style::new();
    apply(
        &mut light,
        "background",
        &bg,
        &tokens_for(&sheet, ThemeKind::Light),
    );
    let mut dark = Style::new();
    apply(
        &mut dark,
        "background",
        &bg,
        &tokens_for(&sheet, ThemeKind::Dark),
    );

    assert!(light.background.is_some());
    assert!(dark.background.is_some());
    assert_ne!(light.background, dark.background, "themes must differ");
}
