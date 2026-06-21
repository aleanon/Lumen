//! The blur primitive + the `BackdropFilter` draw command (glass).

use kurbo::Rect;
use lumen_core::Color;
use lumen_render::cpu;
use lumen_render::display_list::*;
use lumen_render::RgbaImage;

fn px(img: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * img.width() + x) * 4) as usize;
    let p = &img.pixels()[i..i + 4];
    [p[0], p[1], p[2], p[3]]
}

#[test]
fn blur_spreads_a_hard_edge() {
    // Left half black, right half white. The seam column is a hard 0/255 step.
    let raw: Vec<u8> = (0..8)
        .flat_map(|_| {
            (0..40).flat_map(|x| {
                let v = if x < 20 { 0 } else { 255 };
                [v, v, v, 255]
            })
        })
        .collect();
    let a = RgbaImage::from_raw(40, 8, raw);

    let b = a.blurred(4);
    // A pixel just left of the seam was pure black; after blur it picks up light.
    let left = px(&b, 18, 4)[0];
    let right = px(&b, 21, 4)[0];
    assert!(left > 0 && left < 255, "left of seam blended: {left}");
    assert!(right > 0 && right < 255, "right of seam blended: {right}");
    // Far edges stay near their original extreme.
    assert!(px(&b, 0, 4)[0] < 40, "far black stays dark");
    assert!(px(&b, 39, 4)[0] > 215, "far white stays light");
}

#[test]
fn backdrop_filter_blurs_the_painted_backdrop() {
    // Black left, white right, with a hard seam at x = 30.
    let mut dl = DisplayList::new();
    dl.push(DrawCmd::Rect {
        rect: Rect::new(0.0, 0.0, 30.0, 40.0),
        brush: Brush::Solid(Color::BLACK),
        radii: CornerRadii::ZERO,
        border: None,
    });
    dl.push(DrawCmd::Rect {
        rect: Rect::new(30.0, 0.0, 60.0, 40.0),
        brush: Brush::Solid(Color::WHITE),
        radii: CornerRadii::ZERO,
        border: None,
    });

    // Without a filter the seam is a hard step.
    let plain = cpu::render(&dl, 60, 40, Color::WHITE);
    let s_plain = px(&plain, 29, 20)[0];
    assert!(s_plain < 10, "seam is hard without filter: {s_plain}");

    // Add a backdrop filter over a centred region straddling the seam.
    dl.push(DrawCmd::BackdropFilter {
        rect: Rect::new(15.0, 8.0, 45.0, 32.0),
        radii: CornerRadii::ZERO,
        blur: 5.0,
        saturate: 1.0,
    });
    let glass = cpu::render(&dl, 60, 40, Color::WHITE);
    // Inside the filtered region the seam is now a soft ramp (mid grey appears).
    let s_glass = px(&glass, 29, 20)[0];
    assert!(
        s_glass > 40 && s_glass < 215,
        "seam softened by backdrop blur: {s_glass} (was {s_plain})"
    );
    // Outside the filtered region the seam is untouched.
    let outside = px(&glass, 29, 2)[0];
    assert!(outside < 10, "seam outside filter still hard: {outside}");
}
