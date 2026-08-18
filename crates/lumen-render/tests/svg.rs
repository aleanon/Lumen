//! T6.2: minimal SVG renders to deterministic pixels (golden + shape checks).
use lumen_core::Color;
use lumen_render::{svg, RgbaImage};
use std::path::PathBuf;

const SVG: &str = r##"<svg width="40" height="40">
  <rect x="2" y="2" width="16" height="16" fill="#1a73e8"/>
  <circle cx="30" cy="10" r="8" fill="#2ea043"/>
  <path d="M 4 38 L 20 22 L 36 38 Z" fill="#e8404b"/>
</svg>"##;

fn golden(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/cpu")
        .join(format!("{name}.png"))
}

#[test]
fn svg_renders_shapes() {
    let img = svg::render(SVG, 40, 40, Color::WHITE);
    assert_eq!((img.width(), img.height()), (40, 40));

    // The blue rect region is blue-ish; a white corner stays white.
    let px = |x: u32, y: u32| {
        let i = ((y * 40 + x) * 4) as usize;
        let p = img.pixels();
        [p[i], p[i + 1], p[i + 2]]
    };
    let [r, g, b] = px(8, 8);
    assert!(b > 150 && r < 120, "blue rect at (8,8): {r},{g},{b}");
    assert_eq!(px(0, 39), [255, 255, 255], "bottom-left corner white");

    // Golden (exact; CPU renderer is deterministic).
    let path = golden("svg_shapes");
    if std::env::var_os("LUMEN_UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, img.to_png()).unwrap();
        return;
    }
    let expected = RgbaImage::from_png(
        &std::fs::read(&path)
            .unwrap_or_else(|_| panic!("missing {path:?}; run LUMEN_UPDATE_GOLDENS=1")),
    )
    .unwrap();
    assert_eq!(img.diff_count(&expected), 0, "svg golden mismatch");
}

// --- M.2: the completed subset ------------------------------------------------

#[test]
fn m2_groups_transforms_inherit() {
    // A group translated +20,+20 with an inherited fill; the child rect at
    // (0,0,10,10) must paint at (20,20).
    let src = r##"<svg width="40" height="40">
      <g transform="translate(20 20)" fill="#e8404b">
        <rect x="0" y="0" width="10" height="10"/>
      </g>
    </svg>"##;
    let img = svg::render(src, 40, 40, Color::WHITE);
    let px = |x: u32, y: u32| {
        let i = ((y * 40 + x) * 4) as usize;
        let p = img.pixels();
        [p[i], p[i + 1], p[i + 2]]
    };
    let [r, _, b] = px(25, 25);
    assert!(r > 150 && b < 120, "translated red rect at (25,25)");
    let [r0, _, _] = px(5, 5);
    assert!(r0 > 200, "origin stays background");
}

#[test]
fn m2_scale_rotate_matrix_compose() {
    let src = r##"<svg width="40" height="40">
      <rect transform="translate(20 20) rotate(45) scale(2)" x="-2" y="-2"
            width="4" height="4" fill="#1a73e8"/>
    </svg>"##;
    let img = svg::render(src, 40, 40, Color::WHITE);
    let p = img.pixels();
    let i = ((20 * 40 + 20) * 4) as usize;
    assert!(p[i + 2] > 150, "center painted through composed transform");
}

#[test]
fn m2_linear_and_radial_gradients() {
    let src = r##"<svg width="40" height="20">
      <defs>
        <linearGradient id="lg" x1="0" y1="0" x2="40" y2="0">
          <stop offset="0" stop-color="#ff0000"/>
          <stop offset="1" stop-color="#0000ff"/>
        </linearGradient>
      </defs>
      <rect x="0" y="0" width="40" height="20" fill="url(#lg)"/>
    </svg>"##;
    let img = svg::render(src, 40, 20, Color::WHITE);
    let px = |x: u32| {
        let i = ((10 * 40 + x) * 4) as usize;
        let p = img.pixels();
        [p[i], p[i + 1], p[i + 2]]
    };
    let [r_left, _, b_left] = px(2);
    let [r_right, _, b_right] = px(37);
    assert!(
        r_left > 180 && b_left < 100,
        "left end red: {r_left},{b_left}"
    );
    assert!(b_right > 180 && r_right < 100, "right end blue");
}

#[test]
fn m2_clip_rect_masks_children() {
    let src = r##"<svg width="40" height="40">
      <defs><clipPath id="c"><rect x="0" y="0" width="20" height="40"/></clipPath></defs>
      <g clip-path="url(#c)">
        <rect x="0" y="0" width="40" height="40" fill="#2ea043"/>
      </g>
    </svg>"##;
    let img = svg::render(src, 40, 40, Color::WHITE);
    let px = |x: u32| {
        let i = ((20 * 40 + x) * 4) as usize;
        let p = img.pixels();
        [p[i], p[i + 1], p[i + 2]]
    };
    let [_, g_in, _] = px(10);
    let [r_out, g_out, b_out] = px(30);
    assert!(g_in > 120, "inside the clip is green");
    assert!(
        r_out > 200 && g_out > 200 && b_out > 200,
        "outside the clip stays white: {r_out},{g_out},{b_out}"
    );
}

#[test]
fn m2_relative_paths_arcs_polygons() {
    // Relative commands + h/v + an arc + a polygon parse and paint.
    let src = r##"<svg width="40" height="40">
      <path d="m 5 5 l 10 0 v 10 h -10 z" fill="#e8404b"/>
      <path d="M 25 30 A 5 5 0 1 1 35 30" fill="none" stroke="#1a73e8" stroke-width="2"/>
      <polygon points="5,35 15,25 25,35" fill="#2ea043"/>
    </svg>"##;
    let img = svg::render(src, 40, 40, Color::WHITE);
    let p = img.pixels();
    let at = |x: u32, y: u32| {
        let i = ((y * 40 + x) * 4) as usize;
        [p[i], p[i + 1], p[i + 2]]
    };
    assert!(at(10, 10)[0] > 150, "relative-path square filled");
    assert!(at(14, 33)[1] > 100, "polygon filled");
    // The arc strokes somewhere along the top of the half-circle.
    let arc_hit = (25..36).any(|x| (22..30).any(|y| at(x, y)[2] > 150));
    assert!(arc_hit, "arc stroked");
}

#[test]
fn m2_text_goes_through_the_shaper_callback() {
    use lumen_render::{GlyphImage, GlyphRun, PlacedGlyph};
    let src = r##"<svg width="60" height="20">
      <text x="4" y="14" font-size="12" fill="#1a73e8">Hi</text>
    </svg>"##;
    let mut seen: Vec<(String, f64)> = Vec::new();
    let dl = svg::parse_with_text(src, &mut |spec| {
        seen.push((spec.text.to_string(), spec.size));
        // A fake 2x2 glyph so the run flows through the list.
        Some((
            GlyphRun {
                glyphs: vec![PlacedGlyph {
                    image: 0,
                    x: spec.pos.x as f32,
                    y: spec.pos.y as f32,
                    w: 2.0,
                    h: 2.0,
                }],
            },
            vec![GlyphImage {
                key: 1,
                width: 2,
                height: 2,
                coverage: vec![255; 4],
            }],
            kurbo::Rect::new(4.0, 4.0, 20.0, 16.0),
        ))
    });
    assert_eq!(seen, vec![("Hi".to_string(), 12.0)]);
    assert_eq!(dl.runs.len(), 1);
    assert_eq!(dl.glyph_images.len(), 1);
}

// ---------------------------------------------------------------------------
// Nesting depth. SVG is the one asset format the framework parses itself from
// app-supplied bytes, and `walk`/`collect_defs`/`Node::drop` all recurse over
// the parsed tree. Measured before the cap: 256 levels rendered fine, 512
// ABORTED the process — a stack overflow is SIGABRT, not a panic, so neither
// the error boundary nor `rebuild_inner`'s catch_unwind can contain it.
// ---------------------------------------------------------------------------

const SVG_OPEN: &str = r#"<svg viewBox="0 0 100 100" width="100" height="100">"#;

fn nested_groups(depth: usize, inner: &str) -> String {
    let mut s = String::from(SVG_OPEN);
    for _ in 0..depth {
        s.push_str("<g>");
    }
    s.push_str(inner);
    for _ in 0..depth {
        s.push_str("</g>");
    }
    s.push_str("</svg>");
    s
}

const RECT: &str = r##"<rect x="10" y="10" width="50" height="50" fill="#f00"/>"##;

#[test]
fn realistically_nested_groups_still_render() {
    // Real exporters nest tens of levels; the cap must not touch them.
    let dl = svg::parse(&nested_groups(32, RECT));
    assert_eq!(
        dl.cmds.len(),
        1,
        "a rect 32 groups deep must still be drawn"
    );
}

/// Pin the boundary itself. Without this the cap could be lowered to 8 or
/// raised to 4096 — the latter being the tempting "fix" when someone reports a
/// legitimate file truncated, and the one that reopens the overflow — and
/// nothing in the suite would notice.
///
/// 62 rather than 64 because the depth budget counts the whole chain: `<svg>`
/// takes one level and the `<rect>` itself takes the last.
#[test]
fn the_depth_boundary_is_exactly_where_it_claims_to_be() {
    assert_eq!(
        svg::parse(&nested_groups(62, RECT)).cmds.len(),
        1,
        "62 groups is the deepest that still renders"
    );
    assert_eq!(
        svg::parse(&nested_groups(63, RECT)).cmds.len(),
        0,
        "63 groups must be past the cap"
    );
}

/// Fault tolerance, which the depth guard must not cost us. The parser is
/// documented as tolerant of malformed input, and the first version of this
/// cap broke that: it counted "opens I skipped" and decremented on any close,
/// so a closing tag it could not attribute left the counter permanently skewed
/// and silently swallowed everything after it. Bounding ATTACHMENT instead of
/// the parse stack keeps the push/pop sequence identical to the pre-cap
/// parser, and these three inputs are how that is checked.
#[test]
fn depth_truncation_recovers_and_survives_malformed_input() {
    // (a) a deep region that closes properly, then genuinely shallow content
    let mut a = String::from(SVG_OPEN);
    for _ in 0..70 {
        a.push_str("<g>");
    }
    for _ in 0..70 {
        a.push_str("</g>");
    }
    a.push_str(RECT);
    a.push_str("</svg>");
    assert_eq!(
        svg::parse(&a).cmds.len(),
        1,
        "once nesting returns below the cap, content must render again — a          truncated region must not poison the rest of the document"
    );

    // (b) a stray closing tag with no matching open, inside the deep region
    let mut b = String::from(SVG_OPEN);
    for _ in 0..70 {
        b.push_str("<g>");
    }
    b.push_str("</x>");
    for _ in 0..69 {
        b.push_str("</g>");
    }
    b.push_str(RECT);
    b.push_str("</svg>");
    assert_eq!(
        svg::parse(&b).cmds.len(),
        1,
        "a stray close must not desynchronise the depth accounting"
    );

    // (c) unclosed opens: the rect really IS at depth 71 here, so dropping it
    //     is correct — asserted so the behaviour is deliberate, not incidental.
    let mut c = String::from(SVG_OPEN);
    for _ in 0..70 {
        c.push_str("<g>");
    }
    c.push_str("</svg>");
    c.push_str(RECT);
    assert_eq!(
        svg::parse(&c).cmds.len(),
        0,
        "after 70 UNCLOSED groups the rect is nested 71 deep, not shallow"
    );
}

#[test]
fn pathological_nesting_is_truncated_instead_of_aborting_the_process() {
    // Before the depth cap this did not fail — it killed the test binary with
    // `fatal runtime error: stack overflow`. Reaching the assertion at all is
    // most of what this test asserts.
    let dl = svg::parse(&nested_groups(5000, RECT));
    assert!(
        dl.cmds.is_empty(),
        "content nested past the cap must be dropped, not rendered at the \
         wrong transform; got {} cmds",
        dl.cmds.len()
    );
}

#[test]
fn truncating_deep_content_does_not_discard_shallow_siblings() {
    // The over-deep subtree must not corrupt the tree around it: its closing
    // tags have to be balanced against elements that were never kept.
    let src = format!(
        r#"<svg viewBox="0 0 100 100" width="100" height="100">{}{}</svg>"#,
        nested_groups(5000, RECT)
            .replace(
                r#"<svg viewBox="0 0 100 100" width="100" height="100">"#,
                ""
            )
            .replace("</svg>", ""),
        RECT
    );
    let dl = svg::parse(&src);
    assert_eq!(
        dl.cmds.len(),
        1,
        "the shallow sibling rect must survive its over-deep neighbour"
    );
}
