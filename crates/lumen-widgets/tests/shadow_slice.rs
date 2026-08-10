//! The 9-sliced shadow must be BYTE-IDENTICAL to the full-size sprite.
//!
//! A box-shadow used to rasterize one CPU sprite the size of the element plus
//! blur margin, so a 12 016 px card produced a 12 016 px texture and panicked in
//! `create_texture`. The sprite is now sized by style: `blurred` runs three box
//! passes of radius `blur`, so beyond `radius + spread + 3*blur` from a corner
//! the blurred edge stops changing along that edge and one strip of it repeats.
//! Stretching that strip is exact, not an approximation.
//!
//! "Exact" is the whole claim, so it is tested directly rather than inferred.
//! **No golden in the suite has a shadowed element past the threshold** — the
//! sliced path fired zero times across 385 suites when measured — so the
//! goldens passing said nothing about it. `LUMEN_NO_SHADOW_SLICE` forces the
//! old path and these tests compare the two.

use kurbo::Size;
use lumen_app::element::Shadow;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, BuildCx, Element};

/// A shadowed box `w`x`h`. Both axes exceed `2*(radius + 3*blur) + 1 = 133` for
/// the default soft shadow, so both are sliced.
fn app(w: f32, h: f32) -> impl Fn(&mut BuildCx) -> Element {
    move |_cx: &mut BuildCx| {
        let mut el = widgets::column(vec![]);
        el.style.width = Dim::px(w);
        el.style.height = Dim::px(h);
        el.background = Some(lumen_core::Color::srgb8(0xff, 0xff, 0xff, 0xff));
        el.corner_radius = 12.0;
        el.shadow = Some(Shadow::soft());
        let mut root = widgets::column(vec![el.id("box")]);
        root.style.padding = lumen_layout::Edges::all(Dim::px(60.0));
        root.id("root")
    }
}

/// Bounding box of differing pixels, for a failure message that says WHERE.
fn diff_bbox(a: &[u8], b: &[u8], w: usize) -> (usize, usize, usize, usize, usize) {
    let (mut x0, mut y0, mut x1, mut y1, mut n) = (usize::MAX, usize::MAX, 0, 0, 0);
    for (i, (pa, pb)) in a.chunks_exact(4).zip(b.chunks_exact(4)).enumerate() {
        if pa != pb {
            let (x, y) = (i % w, i / w);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            n += 1;
        }
    }
    (x0, y0, x1, y1, n)
}

fn render(w: f32, h: f32, sliced: bool) -> Vec<u8> {
    if sliced {
        std::env::remove_var("LUMEN_NO_SHADOW_SLICE");
    } else {
        std::env::set_var("LUMEN_NO_SHADOW_SLICE", "1");
    }
    let mut h_app =
        App::new(app(w, h)).run_headless(Size::new((w + 140.0) as f64, (h + 140.0) as f64));
    h_app.pump();
    let img = h_app.screenshot();
    std::env::remove_var("LUMEN_NO_SHADOW_SLICE");
    img.pixels().to_vec()
}

/// Both axes sliced.
#[test]
fn a_sliced_shadow_matches_the_full_sprite() {
    let sliced = render(400.0, 300.0, true);
    let full = render(400.0, 300.0, false);
    let (x0, y0, x1, y1, diff) = diff_bbox(&sliced, &full, 540);
    assert_eq!(
        diff, 0,
        "{diff} px differ between the 9-sliced shadow and the full-size sprite, \
         in x {x0}..{x1}, y {y0}..{y1} (frame 540x440, box at 60,60 size \
         400x300). The slice claims to be exact."
    );
}

/// One axis long, one short: the short axis must stay unsliced while the long
/// one stretches. This is the case where an axis-independent bug hides.
#[test]
fn a_shadow_sliced_on_one_axis_only_matches() {
    let sliced = render(120.0, 900.0, true);
    let full = render(120.0, 900.0, false);
    let diff = sliced
        .chunks_exact(4)
        .zip(full.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        diff, 0,
        "{diff} px differ with only the vertical axis sliced"
    );
}
