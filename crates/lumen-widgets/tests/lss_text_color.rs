//! Regression: a `.lss` text `color` must reach the glyphs, not just the
//! computed cascade (the `styling` example rendered its title black before).

use lumen_core::geometry::Size;
use lumen_widgets::{widgets, App, BuildCx};

#[test]
fn lss_color_paints_the_glyphs() {
    // #t is plain black text by default; the stylesheet recolours it accent blue.
    let mut a = App::new(|_cx: &mut BuildCx| widgets::text("XXXX").id("t"))
        .stylesheet("#t { color: #1a73e8ff; }")
        .run_headless(Size::new(120.0, 48.0));
    a.pump();

    let img = a.screenshot();
    // Accent #1a73e8 is strongly blue-dominant; black text never is.
    let blue = img
        .pixels()
        .chunks_exact(4)
        .any(|p| p[2] as i32 > p[0] as i32 + 50 && p[2] > 110);
    assert!(blue, "the .lss text colour reached the rasterized glyphs");
}
