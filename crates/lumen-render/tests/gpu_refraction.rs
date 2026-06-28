//! R3-style 2b: GPU backdrop refraction bends the edge but not the deep
//! interior, mirroring the CPU path. Self-skips without a GPU adapter.
#![cfg(feature = "wgpu")]

use kurbo::Rect;
use lumen_core::Color;
use lumen_render::gpu::Wgpu;
use lumen_render::{Brush, CornerRadii, DisplayList, DrawCmd};

const W: u32 = 120;
const H: u32 = 90;

fn px(img: &lumen_render::RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * img.width() + x) * 4) as usize;
    let p = &img.pixels()[i..i + 4];
    [p[0], p[1], p[2], p[3]]
}

fn scene(refraction: f32) -> DisplayList {
    let mut dl = DisplayList::new();
    for i in 0..30 {
        let x = i as f64 * 4.0;
        dl.push(DrawCmd::Rect {
            rect: Rect::new(x, 0.0, x + 2.0, H as f64),
            brush: Brush::Solid(Color::BLACK),
            radii: CornerRadii::ZERO,
            border: None,
        });
    }
    dl.push(DrawCmd::BackdropFilter {
        rect: Rect::new(15.0, 15.0, 105.0, 75.0),
        radii: CornerRadii::all(16.0),
        blur: 0.0,
        saturate: 1.0,
        refraction,
        specular: 0.0,
    });
    dl
}

#[test]
fn gpu_refraction_bends_edges_not_center() {
    let Some(gpu) = Wgpu::new() else {
        eprintln!("gpu_refraction: no wgpu adapter; skipping");
        return;
    };
    let off = gpu.render(&scene(0.0), W, H, Color::WHITE);
    let on = gpu.render(&scene(8.0), W, H, Color::WHITE);

    // Deep interior is beyond the lens band — unchanged by refraction.
    assert_eq!(
        px(&on, 60, 45),
        px(&off, 60, 45),
        "interior must be untouched"
    );
    // Edge pixels are bent.
    let differ = (15..105)
        .flat_map(|x| (15..75).map(move |y| (x, y)))
        .filter(|&(x, y)| px(&on, x, y) != px(&off, x, y))
        .count();
    assert!(
        differ > 200,
        "refraction should bend edge pixels (differ={differ})"
    );
}
