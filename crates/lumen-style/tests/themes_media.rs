//! T1.3 acceptance: media-query fixtures at 3 window sizes, and a theme switch
//! that changes resolved colors.

use lumen_style::{
    apply, parse, resolve_media, tokens_for, MediaContext, NodeDesc, Origin, Style, StyleSource,
    ThemeKind, Unit, Value,
};

fn button() -> NodeDesc {
    NodeDesc {
        ty: "button".into(),
        ..Default::default()
    }
}

fn app(src: &str) -> StyleSource {
    StyleSource {
        origin: Origin::App,
        sheet: parse("a.lss", src).0,
    }
}

#[test]
fn media_queries_select_rules_at_three_sizes() {
    let sources = [app(r#"
        button { padding: 4px; }
        @media (width >= 600px) { button { padding: 8px; } }
        @media (width >= 1000px) { button { padding: 12px; } }
    "#)];

    for (w, expect) in [(400.0, 4.0), (800.0, 8.0), (1200.0, 12.0)] {
        let ctx = MediaContext {
            width: w,
            ..Default::default()
        };
        let r = resolve_media(&sources, &button(), &ctx);
        assert_eq!(
            r["padding"].value,
            Value::Number(expect, Unit::Px),
            "at width {w}"
        );
    }
}

#[test]
fn media_query_and_combines() {
    let sources = [app(
        "@media (width >= 600px) and (pointer: touch) { button { padding: 20px; } }",
    )];
    let touch = MediaContext {
        width: 800.0,
        pointer: "touch".into(),
        ..Default::default()
    };
    let mouse = MediaContext {
        width: 800.0,
        pointer: "mouse".into(),
        ..Default::default()
    };
    assert!(resolve_media(&sources, &button(), &touch).contains_key("padding"));
    assert!(!resolve_media(&sources, &button(), &mouse).contains_key("padding"));
}

#[test]
fn theme_switch_changes_colors() {
    let sources = [app(r#"
        @theme light { primary: #ffffffff; }
        @theme dark  { primary: #101418ff; }
        button { background: $primary; }
    "#)];
    let ctx = MediaContext::default();
    let r = resolve_media(&sources, &button(), &ctx);
    let bg = r["background"].value.clone();

    let sheet = &sources[0].sheet;
    let mut light = Style::new();
    apply(
        &mut light,
        "background",
        &bg,
        &tokens_for(sheet, ThemeKind::Light),
    );
    let mut dark = Style::new();
    apply(
        &mut dark,
        "background",
        &bg,
        &tokens_for(sheet, ThemeKind::Dark),
    );

    assert_eq!(
        light.background,
        Some(lumen_core::Color::from_hex("#ffffffff").unwrap())
    );
    assert_eq!(
        dark.background,
        Some(lumen_core::Color::from_hex("#101418ff").unwrap())
    );
    assert_ne!(light.background, dark.background);
}
