//! Item 2: screenshot_zoom renders a magnified crop of a region so a small
//! visual defect can be inspected at scale.

use kurbo::{Rect, Size};
use lumen_widgets::{App, Label};

#[test]
fn screenshot_zoom_magnifies_the_region() {
    let mut h = App::new(|_| Label::new("Hi gypq").into()).run_headless(Size::new(120.0, 50.0));
    // Headless scale is 1.0, so zoom == scale_mul. A 20×12 region at 4× → 80×48.
    let img = h.screenshot_zoom(Rect::new(0.0, 0.0, 20.0, 12.0), 4.0, &[]);
    assert_eq!(img.width(), 80, "zoomed width");
    assert_eq!(img.height(), 48, "zoomed height");
}

#[test]
fn screenshot_zoom_with_outlines_still_renders() {
    let mut h = App::new(|_| Label::new("gypq").into()).run_headless(Size::new(120.0, 50.0));
    let box_r = Rect::new(0.0, 0.0, 40.0, 20.0);
    let img = h.screenshot_zoom(
        box_r,
        6.0,
        &[(box_r, lumen_core::Color::srgb8(255, 0, 0, 255))],
    );
    // Non-blank (the outline + text paint something).
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[0] < 250 || p[1] < 250 || p[2] < 250),
        "overlaid zoom should render content"
    );
}
