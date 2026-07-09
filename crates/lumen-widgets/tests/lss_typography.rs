//! B.4a (docs/plan-remediation-2026-07.md): `.lss` typography reaches the
//! text stack — `font-size` resizes the measured box, `font-weight` adds
//! ink (synthesized bold on the single bundled face), `line-height` opens
//! the leading. Previously applied into the computed style but unread.

use kurbo::Size;
use lumen_widgets::{col, widgets, App};

#[test]
fn font_size_resizes_the_measured_text() {
    let mut h = App::new(|_cx| {
        col![
            widgets::text("Hello").id("big"),
            widgets::text("Hello").id("small"),
        ]
    })
    .stylesheet("#big { font-size: 32px; }")
    .run_headless(Size::new(400.0, 300.0));
    h.pump();
    let big = h.node_bounds_by_id("big").unwrap();
    let small = h.node_bounds_by_id("small").unwrap();
    assert!(
        big.height() > small.height() * 1.5,
        "32px text is much taller than the default: {big:?} vs {small:?}"
    );
    assert!(
        big.width() > small.width() * 1.5,
        "and wider: {big:?} vs {small:?}"
    );
    h.assert_view_coherent();
}

#[test]
fn font_weight_adds_ink() {
    let mut h = App::new(|_cx| {
        col![
            widgets::text("Hello").id("bold"),
            widgets::text("Hello").id("reg"),
        ]
    })
    .stylesheet("#bold { font-weight: 900; }")
    .run_headless(Size::new(400.0, 300.0));
    h.pump();
    let shot = h.screenshot();
    let ink = |b: kurbo::Rect| {
        let mut n = 0u32;
        for y in b.y0 as u32..b.y1 as u32 {
            for x in b.x0 as u32..b.x1 as u32 {
                let p = shot.pixel(x, y);
                // Count non-background pixels (any visible glyph coverage).
                if p[3] > 0 && (p[0] as u16 + p[1] as u16 + p[2] as u16) < 600 {
                    n += 1;
                }
            }
        }
        n
    };
    let bold = ink(h.node_bounds_by_id("bold").unwrap());
    let reg = ink(h.node_bounds_by_id("reg").unwrap());
    assert!(
        bold > reg,
        "weight 900 synthesizes bolder glyphs: {bold} vs {reg} ink px"
    );
}

#[test]
fn line_height_opens_the_leading() {
    // A wrapped two-line paragraph: line-height 2 makes the block taller.
    let text = "a somewhat long label that wraps to two lines here";
    let mut h = App::new(move |_cx| {
        col![
            widgets::text(text).id("airy"),
            widgets::text(text).id("dense"),
        ]
    })
    .stylesheet("#airy { width: 160px; line-height: 2; } #dense { width: 160px; }")
    .run_headless(Size::new(400.0, 400.0));
    h.pump();
    let airy = h.node_bounds_by_id("airy").unwrap();
    let dense = h.node_bounds_by_id("dense").unwrap();
    assert!(
        airy.height() > dense.height() * 1.4,
        "line-height 2 opens the leading: {airy:?} vs {dense:?}"
    );
    h.assert_view_coherent();
}
