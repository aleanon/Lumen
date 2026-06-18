//! Design-analysis acceptance: deterministic APCA text-contrast report computed
//! from the display list, compared against a JSON golden.
//!
//! This is the end-to-end de-risking slice for the "critique-as-data" proposal:
//! a scene → a structured, node-addressable contrast report → a stable golden.
//!
//! The golden lives at `tests/golden/design/contrast.json`. Re-record with
//! `LUMEN_UPDATE_GOLDENS=1 cargo test -p lumen-render`. CI never sets it.

use kurbo::{Affine, Point, Rect};
use lumen_core::Color;
use lumen_render::analysis::{analyze_contrast, ContrastLevel, TextTarget};
use lumen_render::display_list::*;
use std::path::PathBuf;

fn golden_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/design/contrast.json")
}

/// A representative screen: a near-opaque dark "card" floating on a dark page,
/// with three text runs of varying legibility painted on top.
fn scene() -> (DisplayList, Color, Vec<TextTarget>) {
    let page_bg = Color::srgb8(0x12, 0x14, 0x18, 0xff); // near-black page
    let mut dl = DisplayList::new();

    // A card at 92% opacity so the resolved backdrop is *composited*, not the
    // card's nominal color — exactly the case a screenshot-sampler gets wrong.
    dl.push(DrawCmd::PushLayer {
        clip: Some(RoundedRect {
            rect: Rect::new(40.0, 40.0, 360.0, 280.0),
            radii: CornerRadii::all(16.0),
        }),
        opacity: 0.92,
        transform: Affine::IDENTITY,
        blend: BlendMode::SourceOver,
    });
    dl.push(DrawCmd::Rect {
        rect: Rect::new(40.0, 40.0, 360.0, 280.0),
        brush: Brush::Solid(Color::srgb8(0x24, 0x28, 0x30, 0xff)),
        radii: CornerRadii::all(16.0),
        border: None,
    });
    dl.push(DrawCmd::PopLayer);

    // Text runs are GlyphRuns in the real pipeline; here we describe them
    // directly (foreground + region). Their colors stand in for the brush.
    let targets = vec![
        TextTarget {
            node: Some("node-11".into()),
            label: Some("Heading (near-white)".into()),
            foreground: Color::srgb8(0xf2, 0xf4, 0xf8, 0xff),
            region: Rect::new(64.0, 64.0, 320.0, 96.0),
        },
        TextTarget {
            node: Some("node-12".into()),
            label: Some("Body (mid-gray)".into()),
            foreground: Color::srgb8(0x9a, 0xa0, 0xaa, 0xff),
            region: Rect::new(64.0, 120.0, 320.0, 150.0),
        },
        TextTarget {
            node: Some("node-13".into()),
            label: Some("Muted caption (too dim)".into()),
            foreground: Color::srgb8(0x44, 0x4a, 0x55, 0xff),
            region: Rect::new(64.0, 220.0, 320.0, 250.0),
        },
    ];
    (dl, page_bg, targets)
}

#[test]
fn contrast_report_matches_golden() {
    let (dl, page_bg, targets) = scene();
    let report = analyze_contrast(&dl, page_bg, &targets);
    let json = serde_json::to_string_pretty(&report).unwrap();

    let path = golden_file();
    if std::env::var_os("LUMEN_UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("{json}\n")).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing golden {path:?}; run with LUMEN_UPDATE_GOLDENS=1"));
    assert_eq!(
        json.trim(),
        expected.trim(),
        "contrast report drifted from golden {path:?}"
    );
}

/// Spot-check the semantics independently of the byte-golden so a regression
/// reads clearly: the muted caption must be flagged, the heading must pass.
#[test]
fn verdicts_are_actionable() {
    let (dl, page_bg, targets) = scene();
    let report = analyze_contrast(&dl, page_bg, &targets);

    let heading = &report.targets[0];
    let caption = &report.targets[2];

    assert!(
        heading.passes_body_text,
        "near-white heading should clear body text, got Lc {}",
        heading.apca_lc
    );
    assert_eq!(caption.level, ContrastLevel::Fail);
    assert!(
        !caption.passes_body_text,
        "muted caption should fail, got Lc {}",
        caption.apca_lc
    );
    // Every finding is bound to a node the agent can act on.
    assert!(report.targets.iter().all(|t| t.node.is_some()));
}

/// The resolved background must reflect the composited card, not the page.
#[test]
fn background_is_the_composited_card() {
    let (dl, page_bg, _targets) = scene();
    let bg = lumen_render::analysis::resolve_backdrop(&dl, page_bg, Point::new(200.0, 160.0));
    // Card #242830 at 92% over page #121418 → between the two, nearer the card.
    assert_ne!(
        bg.to_hex(),
        "#242830ff",
        "should be composited, not nominal"
    );
    assert_ne!(bg.to_hex(), "#121418ff", "should not be the bare page");
}
