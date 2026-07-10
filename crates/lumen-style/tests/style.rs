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

/// Assert the `.lss` value and the typed setter produce the same `Style`,
/// recording the property in `$covered` for the set-equality check (B.7).
macro_rules! style_parity {
    ($covered:ident, $prop:literal, $lss:literal, $typed:expr) => {{
        let mut from_lss = Style::new();
        apply(&mut from_lss, $prop, &val($prop, $lss), &Tokens::new());
        let from_typed: Style = $typed(Style::new());
        assert_eq!(from_lss, from_typed, "parity for {}: {}", $prop, $lss);
        $covered.push($prop);
    }};
}

#[test]
fn lss_matches_typed_mirror_over_the_whole_applied_set() {
    use lumen_core::Color;
    use lumen_layout::FlexDirection;
    let red = || Color::from_hex("#ff0000ff").unwrap();
    let mut covered: Vec<&str> = Vec::new();
    style_parity!(covered, "background", "#1a73e8ff", |s: Style| s
        .background(Color::from_hex("#1a73e8ff").unwrap()));
    style_parity!(covered, "color", "#ffffffff", |s: Style| s
        .color(Color::from_hex("#ffffffff").unwrap()));
    style_parity!(covered, "padding", "8px", |s: Style| s.padding(8.0));
    style_parity!(covered, "border-radius", "6px", |s: Style| s.radius(6.0));
    style_parity!(covered, "opacity", "0.45", |s: Style| s.opacity(0.45));
    style_parity!(covered, "font-size", "16px", |s: Style| s.font_size(16.0));
    style_parity!(covered, "font-weight", "600", |s: Style| s.font_weight(600));
    style_parity!(covered, "width", "100px", |s: Style| s.width(100.0));
    style_parity!(covered, "gap", "8px", |s: Style| s.gap(8.0));
    style_parity!(covered, "display", "flex", |s: Style| s
        .display(Display::Flex));
    style_parity!(covered, "flex-direction", "column", |s: Style| s
        .flex_direction(FlexDirection::Column));
    style_parity!(covered, "height", "40px", |s: Style| s.height(40.0));
    style_parity!(covered, "margin", "12px", |s: Style| s.margin(12.0));
    style_parity!(covered, "line-height", "1.5", |s: Style| s.line_height(1.5));
    style_parity!(covered, "border", "2px #ff0000ff", |s: Style| s
        .border(2.0, red()));
    style_parity!(covered, "border-width", "3px", |s: Style| s
        .border_width(3.0));
    style_parity!(covered, "border-color", "#ff0000ff", |s: Style| s
        .border_color(red()));
    style_parity!(
        covered,
        "backdrop-filter",
        "blur(4px) saturate(1.8)",
        |s: Style| { s.backdrop_blur(4.0).backdrop_saturate(1.8) }
    );
    style_parity!(covered, "visibility", "hidden", |s: Style| s
        .visibility(false));
    style_parity!(covered, "shadow", "0 2px 8px #00000033", |s: Style| s
        .shadow(lumen_style::StyleShadow {
            dx: 0.0,
            dy: 2.0,
            blur: 8.0,
            spread: 0.0,
            color: Color::from_hex("#00000033").unwrap(),
        }));

    // Set equality (04 §8): the parity table above covers exactly the
    // runtime's applied set — a new `apply` arm without a typed setter (or
    // vice versa) fails here, not silently.
    let mut want: Vec<&str> = lumen_style::APPLIED_PROPERTIES.to_vec();
    want.sort_unstable();
    covered.sort_unstable();
    assert_eq!(covered, want, "parity table != APPLIED_PROPERTIES");
}

#[test]
fn applied_properties_change_a_style_and_only_they_do() {
    // Representative value per applied property.
    let repr = |p: &str| match p {
        "display" => "flex",
        "flex-direction" => "column",
        "background" | "color" | "border-color" => "#ff0000ff",
        "border-radius" => "6px",
        "opacity" => "0.5",
        "font-weight" => "600",
        "line-height" => "1.5",
        "backdrop-filter" => "blur(4px)",
        "shadow" => "0 2px 8px #00000033",
        "visibility" => "hidden",
        "border" => "2px #ff0000ff",
        _ => "8px", // the lengths
    };
    for &p in lumen_style::APPLIED_PROPERTIES {
        let mut s = Style::new();
        apply(&mut s, p, &val(p, repr(p)), &Tokens::new());
        assert_ne!(
            s,
            Style::new(),
            "`{p}` is listed as applied but apply() ignored it"
        );
    }
    // Every other known property must be inert (parse-only) — an arm added
    // to apply() without updating APPLIED_PROPERTIES fails here.
    for &p in lumen_style::KNOWN_PROPERTIES {
        if lumen_style::APPLIED_PROPERTIES.contains(&p) {
            continue;
        }
        let mut s = Style::new();
        for v in ["8px", "flex", "#ff0000ff", "0.5"] {
            let src = format!("x {{ {p}: {v}; }}");
            let (sheet, _) = lumen_style::parse("t.lss", &src);
            if let Item::Rule(r) = &sheet.items[0] {
                if let Some(d) = r.declarations.first() {
                    apply(&mut s, p, &d.value, &Tokens::new());
                }
            }
        }
        assert_eq!(
            s,
            Style::new(),
            "`{p}` changed Style but is not in APPLIED_PROPERTIES"
        );
    }
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
