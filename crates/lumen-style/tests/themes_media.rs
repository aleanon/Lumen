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

#[test]
fn border_shorthand_and_longhands_resolve() {
    let sheet = parse(
        "b.lss",
        r#"
        .a { border: 2px #d8dde3ff; }
        .b { border-width: 3px; border-color: #112233ff; }
    "#,
    )
    .0;
    let sources = [StyleSource {
        origin: Origin::App,
        sheet,
    }];
    let tokens = tokens_for(&sources[0].sheet, ThemeKind::Light);
    let resolve_class = |class: &str| {
        let node = NodeDesc {
            classes: vec![class.into()],
            ..Default::default()
        };
        let r = resolve_media(&sources, &node, &MediaContext::default());
        let mut s = Style::new();
        for (prop, c) in &r {
            apply(&mut s, prop, &c.value, &tokens);
        }
        s
    };

    // Shorthand sets both width and color (order-independent parse).
    let a = resolve_class("a");
    assert_eq!(a.border_width, Some(2.0));
    assert_eq!(
        a.border_color,
        Some(lumen_core::Color::from_hex("#d8dde3ff").unwrap())
    );
    // Longhands set them independently.
    let b = resolve_class("b");
    assert_eq!(b.border_width, Some(3.0));
    assert_eq!(
        b.border_color,
        Some(lumen_core::Color::from_hex("#112233ff").unwrap())
    );
}

#[test]
fn backdrop_filter_parses_all_functions() {
    let sheet = parse(
        "g.lss",
        ".glass { backdrop-filter: blur(18px) saturate(140%) refraction(14px) specular(0.6); }",
    )
    .0;
    let sources = [StyleSource {
        origin: Origin::App,
        sheet,
    }];
    let tokens = tokens_for(&sources[0].sheet, ThemeKind::Light);
    let node = NodeDesc {
        classes: vec!["glass".into()],
        ..Default::default()
    };
    let mut s = Style::new();
    for (prop, c) in resolve_media(&sources, &node, &MediaContext::default()) {
        apply(&mut s, &prop, &c.value, &tokens);
    }
    assert_eq!(s.backdrop_blur, Some(18.0));
    assert_eq!(s.backdrop_saturate, Some(1.4)); // 140% → 1.4
    assert_eq!(s.backdrop_refraction, Some(14.0));
    assert_eq!(s.backdrop_specular, Some(0.6));
}
