//! B.7 (docs/plan-remediation-2026-07.md): relative colors —
//! `oklch(from <color|$token> L C H)` with `l`/`c`/`h` channel keywords and
//! `calc(…)` arithmetic (the 04 §4 hover example). Previously `calc(l + 0.06)`
//! was a parse error (`+` had no token).

use lumen_core::Color;
use lumen_style::{apply, has_errors, parse, tokens_for, Style, ThemeKind};

/// Parse `src`, take the first declaration of `button { … }`, and apply it.
fn styled(src: &str) -> Style {
    let (sheet, diags) = parse("test.lss", src);
    assert!(!has_errors(&diags), "parse errors: {diags:?}");
    let tokens = tokens_for(&sheet, ThemeKind::Light);
    let mut style = Style::default();
    for item in &sheet.items {
        if let lumen_style::Item::Rule(r) = item {
            for d in &r.declarations {
                apply(&mut style, &d.property, &d.value, &tokens);
            }
        }
    }
    style
}

fn lightened(base: Color, dl: f32) -> Color {
    let (l, c, h) = base.to_oklch();
    let mut out = Color::from_oklch(l + dl, c, h);
    out.a = base.a;
    out
}

#[test]
fn calc_plus_lightens_the_base_channel() {
    let s = styled("button { background: oklch(from #3366ff calc(l + 0.06) c h); }");
    let base = Color::from_hex("#3366ff").unwrap();
    assert_eq!(
        s.background.expect("background resolved").to_hex(),
        lightened(base, 0.06).to_hex()
    );
}

#[test]
fn token_base_resolves_inside_the_function() {
    let s = styled(
        "@tokens { primary: #3366ff; }\n\
         button { background: oklch(from $primary calc(l + 0.06) c h); }",
    );
    let base = Color::from_hex("#3366ff").unwrap();
    assert_eq!(
        s.background.expect("background resolved").to_hex(),
        lightened(base, 0.06).to_hex()
    );
}

#[test]
fn keyword_channels_round_trip_the_base() {
    let s = styled("button { background: oklch(from #cc2244 l c h); }");
    let (l, c, h) = Color::from_hex("#cc2244").unwrap().to_oklch();
    assert_eq!(
        s.background.expect("background resolved").to_hex(),
        Color::from_oklch(l, c, h).to_hex()
    );
}

#[test]
fn minus_and_scale_operators_evaluate_left_to_right() {
    let s = styled("button { background: oklch(from #3366ff calc(l - 0.1) calc(c * 0.5) h); }");
    let (l, c, h) = Color::from_hex("#3366ff").unwrap().to_oklch();
    assert_eq!(
        s.background.expect("background resolved").to_hex(),
        Color::from_oklch(l - 0.1, c * 0.5, h).to_hex()
    );
}

#[test]
fn literal_channel_overrides_ignore_the_base() {
    // Absolute L with inherited c/h.
    let s = styled("button { background: oklch(from #3366ff 0.9 c h); }");
    let (_, c, h) = Color::from_hex("#3366ff").unwrap().to_oklch();
    assert_eq!(
        s.background.expect("background resolved").to_hex(),
        Color::from_oklch(0.9, c, h).to_hex()
    );
}

#[test]
fn spec_hover_example_parses_clean() {
    // The exact 04 §4 snippet shape.
    let (_, diags) = parse(
        "test.lss",
        "@tokens { radius: 6px; }\n\
         @theme light { primary: oklch(0.62 0.19 255); bg: #ffffff; }\n\
         button.primary { background: $primary; color: $bg; border-radius: $radius;\n\
           &:hover { background: oklch(from $primary calc(l + 0.06) c h); }\n\
         }",
    );
    assert!(!has_errors(&diags), "spec example must parse: {diags:?}");
}
