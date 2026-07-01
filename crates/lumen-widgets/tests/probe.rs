//! Item 3: pixel probes over a rendered frame (agent ui.probe/ui.probeRegion).

use kurbo::Size;
use lumen_widgets::{App, Label};

#[test]
fn probe_a_rendered_frame() {
    let mut h = App::new(|_| Label::new("Hi there").into()).run_headless(Size::new(120.0, 40.0));
    let img = h.screenshot();
    // The top-left corner is background (near-white).
    let c = img.pixel(1, 1);
    assert!(
        c[0] > 240 && c[1] > 240 && c[2] > 240,
        "corner should be background, got {c:?}"
    );
    // The whole frame is not uniform — the label painted glyphs.
    assert!(
        img.region_is_uniform(0, 0, img.width(), img.height())
            .is_none(),
        "a frame with text must have content"
    );
}
