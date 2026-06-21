use lumen_core::geometry::Size;

#[test]
fn frosted_card_over_vivid_backdrop() {
    let mut a = glass::main_app().run_headless(Size::new(600.0, 520.0));
    a.pump();

    let t = a.semantics_json().to_string();
    assert!(t.contains("Liquid Glass"), "title present");
    assert!(
        t.contains("Frosted") && t.contains("Vibrant") && t.contains("Live"),
        "chips present"
    );

    let img = a.screenshot();
    let px = |x: u32, y: u32| {
        let i = ((y * 600 + x) * 4) as usize;
        let p = &img.pixels()[i..i + 4];
        [p[0], p[1], p[2]]
    };
    // The backdrop is painted vivid: a saturated blob colour exists outside the
    // card (e.g. the magenta top-left).
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[0] > 200 && p[1] < 120 && p[2] > 120),
        "vivid magenta blob painted"
    );
    // The card centre sits over the blurred backdrop + white tint: a soft,
    // light-ish blend rather than a fully-saturated pure blob colour.
    let [r, g, b] = px(300, 250);
    let max = r.max(g).max(b) as i32;
    let min = r.min(g).min(b) as i32;
    assert!(max > 120, "card centre is lightened by the glass tint");
    assert!(
        max - min < 110,
        "card centre is a desaturated blend, not a pure blob"
    );
}
