//! Item 1: the runtime records rendered *ink* bounds per node and flags when ink
//! is clipped by its own box (W0104) — the intent-vs-result check that box-only
//! audits miss (e.g. a too-small line-height clipping descenders).

use kurbo::Size;
use lumen_core::codes;
use lumen_widgets::{App, Label};

fn clip_diags(h: &lumen_widgets::Headless) -> usize {
    h.diagnostics()
        .iter()
        .filter(|d| d.code == codes::W0104)
        .count()
}

#[test]
fn tight_line_height_clips_descenders_and_flags_w0104() {
    // A descender-heavy string at line-height 1.0: the line box is too short to
    // hold the glyph ink, so it's clipped.
    let h = App::new(|_| Label::new("gypq jQ").line_height(1.0).into())
        .run_headless(Size::new(240.0, 80.0));
    assert!(
        clip_diags(&h) >= 1,
        "tight line-height should flag W0104; diags = {:?}",
        h.diagnostics()
    );
}

#[test]
fn default_line_height_does_not_clip() {
    // The default (1.3) reserves room for the full glyph extent → no clipping.
    let h = App::new(|_| Label::new("gypq jQ").into()).run_headless(Size::new(240.0, 80.0));
    assert_eq!(
        clip_diags(&h),
        0,
        "default line-height must not clip; diags = {:?}",
        h.diagnostics()
    );
}
