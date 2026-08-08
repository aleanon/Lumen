//! T1.2 acceptance: `.lss`↔typed `Style` parity, computed-value serialization
//! (04 §7), and light/dark theme-token resolution.

use lumen_layout::Display;
use lumen_style::{apply, tokens_for, Style, ThemeKind, Tokens};
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
    use lumen_layout::{Align, FlexDirection, FlexWrap};
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
    // PROP1's mechanical batch.
    style_parity!(covered, "row-gap", "8px", |s: Style| s.row_gap(8.0));
    style_parity!(covered, "column-gap", "8px", |s: Style| s.column_gap(8.0));
    style_parity!(covered, "justify-content", "center", |s: Style| s
        .justify_content(Align::Center));
    style_parity!(covered, "align-items", "center", |s: Style| s
        .align_items(Align::Center));
    style_parity!(covered, "align-self", "center", |s: Style| s
        .align_self(Align::Center));
    style_parity!(covered, "flex-wrap", "wrap", |s: Style| s
        .flex_wrap(FlexWrap::Wrap));
    style_parity!(covered, "flex-grow", "1", |s: Style| s.flex_grow(1.0));
    style_parity!(covered, "flex-shrink", "1", |s: Style| s.flex_shrink(1.0));
    style_parity!(covered, "min-width", "8px", |s: Style| s.min_width(8.0));
    style_parity!(covered, "min-height", "8px", |s: Style| s.min_height(8.0));
    style_parity!(covered, "max-width", "8px", |s: Style| s.max_width(8.0));
    style_parity!(covered, "max-height", "8px", |s: Style| s.max_height(8.0));
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
    for (i, side) in ["top", "right", "bottom", "left"].iter().enumerate() {
        let prop = format!("padding-{side}");
        let mut from_lss = Style::new();
        apply(&mut from_lss, &prop, &val(&prop, "8px"), &Tokens::new());
        assert_eq!(from_lss, Style::new().padding_side(i, 8.0), "{prop}");
        let prop = format!("margin-{side}");
        let mut from_lss = Style::new();
        apply(&mut from_lss, &prop, &val(&prop, "8px"), &Tokens::new());
        assert_eq!(from_lss, Style::new().margin_side(i, 8.0), "{prop}");
    }
    covered.extend([
        "padding-top",
        "padding-right",
        "padding-bottom",
        "padding-left",
        "margin-top",
        "margin-right",
        "margin-bottom",
        "margin-left",
    ]);
    // PROP1's second batch: layout properties whose `LayoutStyle` fields
    // already existed but which `apply()` never read.
    style_parity!(covered, "flex-basis", "8px", |s: Style| s
        .flex_basis(lumen_layout::Dim::px(8.0)));
    style_parity!(covered, "align-content", "center", |s: Style| s
        .align_content(lumen_layout::Align::Center));
    style_parity!(covered, "aspect-ratio", "1.5", |s: Style| s
        .aspect_ratio(1.5));
    style_parity!(covered, "position", "absolute", |s: Style| s
        .position(lumen_layout::Position::Absolute));
    style_parity!(covered, "inset", "8px", |s: Style| s
        .inset(lumen_layout::Edges::all(lumen_layout::Dim::px(8.0))));
    for (i, side) in ["top", "right", "bottom", "left"].iter().enumerate() {
        let prop = format!("inset-{side}");
        let mut from_lss = Style::new();
        apply(&mut from_lss, &prop, &val(&prop, "8px"), &Tokens::new());
        let mut want = Style::new();
        want.inset_sides[i] = Some(8.0);
        assert_eq!(from_lss, want, "{prop}");
    }
    covered.extend(["inset-top", "inset-right", "inset-bottom", "inset-left"]);
    // `overflow` is the CSS spelling of `clip`; both write the same field.
    style_parity!(covered, "overflow", "hidden", |s: Style| s
        .clip(lumen_style::StyleClip::Rounded));
    style_parity!(covered, "grid-template-columns", "1fr 2fr", |s: Style| s
        .grid_template_columns(vec![
            lumen_layout::GridTrack::Fr(1.0),
            lumen_layout::GridTrack::Fr(2.0),
        ]));
    style_parity!(covered, "grid-template-rows", "1fr 2fr", |s: Style| s
        .grid_template_rows(vec![
            lumen_layout::GridTrack::Fr(1.0),
            lumen_layout::GridTrack::Fr(2.0),
        ]));
    style_parity!(covered, "grid-column", "2", |s: Style| s.grid_column(
        lumen_layout::GridLine::Line(2),
        lumen_layout::GridLine::Auto
    ));
    style_parity!(covered, "grid-row", "2", |s: Style| s.grid_row(
        lumen_layout::GridLine::Line(2),
        lumen_layout::GridLine::Auto
    ));
    style_parity!(covered, "cursor", "pointer", |s: Style| s
        .cursor(lumen_core::CursorShape::Pointer));
    style_parity!(covered, "font-style", "italic", |s: Style| s
        .font_italic(true));
    style_parity!(covered, "text-align", "center", |s: Style| s
        .text_align(lumen_text::TextAlign::Center));
    style_parity!(covered, "letter-spacing", "8px", |s: Style| s
        .letter_spacing(8.0));
    style_parity!(covered, "font-family", "Inter", |s: Style| s
        .font_family("Inter"));
    style_parity!(covered, "visibility", "hidden", |s: Style| s
        .visibility(false));
    style_parity!(covered, "clip", "rounded", |s: Style| s
        .clip(lumen_style::StyleClip::Rounded));
    style_parity!(covered, "blend-mode", "multiply", |s: Style| s
        .blend_mode(lumen_style::StyleBlend::Multiply));
    style_parity!(covered, "animation", "fadein 100ms linear", |s: Style| {
        s.animation(lumen_style::AnimationSpec {
            name: "fadein".into(),
            duration_ms: 100.0,
            easing: lumen_style::Easing::Linear,
            delay_ms: 0.0,
            count: Some(1.0),
            alternate: false,
        })
    });
    style_parity!(covered, "animation-force", "true", |s: Style| s
        .animation_force(true));
    style_parity!(
        covered,
        "transition",
        "background 120ms linear",
        |s: Style| {
            s.transition(lumen_style::Transition {
                property: "background".into(),
                duration_ms: 120.0,
                easing: lumen_style::Easing::Linear,
                delay_ms: 0.0,
            })
        }
    );
    for (i, side) in ["top", "right", "bottom", "left"].iter().enumerate() {
        let prop = format!("border-{side}");
        let mut from_lss = Style::new();
        apply(
            &mut from_lss,
            &prop,
            &val(&prop, "2px #ff0000ff"),
            &Tokens::new(),
        );
        assert_eq!(from_lss, Style::new().border_side(i, 2.0, red()), "{prop}");
    }
    covered.extend(["border-top", "border-right", "border-bottom", "border-left"]);
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
        "clip" => "rounded",
        "blend-mode" => "multiply",
        "transition" => "background 120ms linear",
        "animation" => "fadein 100ms linear",
        "animation-force" => "true",
        "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
            "2px #ff0000ff"
        }
        // PROP1's mechanical batch: keywords and bare numbers need their own
        // samples, since the "8px" fallback is not a valid value for them.
        "justify-content" | "align-items" | "align-self" => "center",
        "flex-wrap" => "wrap",
        "flex-grow" | "flex-shrink" => "1",
        "align-content" => "center",
        "position" => "absolute",
        "aspect-ratio" => "1.5",
        "font-family" => "Inter",
        "text-align" => "center",
        "font-style" => "italic",
        "cursor" => "pointer",
        "grid-template-columns" | "grid-template-rows" => "1fr 2fr",
        "grid-column" | "grid-row" => "2",
        "overflow" => "hidden",
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

/// Snapshot-only: `canonical`/`computed_json` are the JSON export, which lean
/// builds do not compile. The imports live here rather than at module scope so
/// the rest of the file stays profile-agnostic.
#[cfg(feature = "snapshot")]
#[test]
fn computed_value_serialization() {
    use lumen_style::{canonical, computed_json, Origin};

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

/// SD5.x: a known, implemented property given a value the runtime cannot use
/// must report `W0109`. These three were all silent before — they parse, fail
/// their value check, and leave the property unset, with nothing to read.
#[test]
fn an_unusable_value_on_an_implemented_property_reports_w0109() {
    for (prop, bad) in [
        ("text-align", "justify"),
        ("overflow", "scroll"),
        ("display", "flext"),
    ] {
        let src = format!("x {{ {prop}: {bad}; }}");
        let (_, ds) = lumen_style::parse("t.lss", &src);
        assert!(
            ds.iter().any(|d| d.code == lumen_core::codes::W0109),
            "`{prop}: {bad}` must report W0109, got {ds:?}"
        );
    }
}

/// The complement, and the reason the check is narrow: every accepted value
/// must stay silent. A diagnostic that fires on working stylesheets is worse
/// than the silence it replaced.
#[test]
fn accepted_values_do_not_report_w0109() {
    for (prop, good) in [
        ("text-align", "center"),
        ("overflow", "hidden"),
        ("display", "grid"),
        ("align-items", "center"),
        ("flex-wrap", "wrap"),
        ("width", "10px"),
        ("color", "#ff0000ff"),
        ("transition", "background 120ms linear"),
    ] {
        let src = format!("x {{ {prop}: {good}; }}");
        let (_, ds) = lumen_style::parse("t.lss", &src);
        assert!(
            !ds.iter().any(|d| d.code == lumen_core::codes::W0109),
            "`{prop}: {good}` is valid and must not warn, got {ds:?}"
        );
    }
}

/// SD5.x, the numeric half: `aspect-ratio: 0` would collapse the node in taffy,
/// so it is rejected — and must now say so instead of leaving the property
/// quietly unset.
#[test]
fn an_unusable_number_reports_w0109() {
    for bad in ["0", "-1"] {
        let src = format!("x {{ aspect-ratio: {bad}; }}");
        let (_, ds) = lumen_style::parse("t.lss", &src);
        assert!(
            ds.iter().any(|d| d.code == lumen_core::codes::W0109),
            "`aspect-ratio: {bad}` must report W0109, got {ds:?}"
        );
    }
    // Valid ratios and other scalars stay silent.
    for (prop, good) in [
        ("aspect-ratio", "1.5"),
        ("opacity", "0"),
        ("flex-grow", "0"),
        ("line-height", "1.5"),
    ] {
        let src = format!("x {{ {prop}: {good}; }}");
        let (_, ds) = lumen_style::parse("t.lss", &src);
        assert!(
            !ds.iter().any(|d| d.code == lumen_core::codes::W0109),
            "`{prop}: {good}` is valid and must not warn, got {ds:?}"
        );
    }
}
