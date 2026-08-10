//! The device's texture ceiling, and that ordinary large frames do not panic.
//!
//! Lumen used to request `Limits::downlevel_defaults()`, and REQUESTED limits
//! are what wgpu validates against — so `max_texture_dimension_2d` was pinned at
//! 2048 on hardware supporting 8192–32768. Consequences, all of them hard
//! panics rather than errors:
//!
//! * any window over 2048 physical px died in `Surface::configure` at open
//!   (1080p survived, 1440p and 4K did not);
//! * a box-shadow on a tall element produced an oversized sprite and died in
//!   `create_texture`;
//! * an image asset larger than 2048 px did the same.
//!
//! These tests pin the ceiling and exercise a 4K-sized frame, which panicked
//! before. They do NOT cover the shadow case: a 12 016 px sprite exceeds even
//! the raised ceiling, and the fix for that is making the sprite style-sized
//! rather than content-sized. See `gpu_oversize.rs`.
#![cfg(feature = "wgpu")]

mod common;

use common::*;
use kurbo::Rect;
use lumen_core::Color;
use lumen_render::display_list::*;
use lumen_render::gpu::Wgpu;

fn require_gpu() -> Option<Wgpu> {
    match Wgpu::new() {
        Some(g) => Some(g),
        None if std::env::var_os("LUMEN_REQUIRE_GPU").is_some() => {
            panic!("LUMEN_REQUIRE_GPU is set but no wgpu adapter was found")
        }
        None => {
            eprintln!("skipping: no wgpu adapter");
            None
        }
    }
}

/// Any adapter Lumen will actually run on clears 4096. The old value was 2048,
/// which is the WebGL2/downlevel floor and not a property of the hardware.
#[test]
fn the_texture_ceiling_is_not_the_downlevel_floor() {
    let Some(gpu) = require_gpu() else { return };
    let cap = gpu.max_texture_dimension();
    assert!(
        cap >= 4096,
        "{} reports max_texture_dimension {cap}; expected >= 4096. If this fires \
         on real hardware the `using_resolution(adapter.limits())` request was \
         lost, and 2048 is back.",
        gpu.adapter_name()
    );
    assert!(
        cap <= 8192,
        "the ceiling is deliberately capped at 8192: a 16384^2 RGBA texture is \
         1 GiB and the readback buffer follows it, got {cap}"
    );
}

/// A 4K frame. Every offscreen target is viewport-sized, so at the old 2048 cap
/// this panicked inside `create_texture` before drawing anything.
#[test]
fn a_4k_frame_renders_without_panicking() {
    let Some(gpu) = require_gpu() else { return };
    let mut dl = DisplayList::new();
    dl.push(DrawCmd::Rect {
        rect: Rect::new(100.0, 100.0, 3740.0, 2060.0),
        brush: Brush::Solid(Color::srgb8(0x20, 0x60, 0xc0, 0xff)),
        radii: CornerRadii::all(8.0),
        border: None,
    });
    let img = gpu.render_at_scale(&dl, 3840, 2160, 1.0, bg());
    assert_eq!((img.width(), img.height()), (3840, 2160));
    // Non-blank: the rect actually made it through at this size.
    assert!(
        frame_diff(&img, &img).differing == 0 && img.pixels().iter().any(|&b| b != 0),
        "the 4K frame came back empty"
    );
}

/// An image past the ceiling is downscaled rather than handed to wgpu.
///
/// This is the invariant the whole guard exists for: producers upstream size
/// their own sprites sensibly, but `upload_image` is what makes "no oversize
/// texture" provable instead of audited. 9000 px exceeds even the raised
/// ceiling, so nothing but the backstop can save it.
#[test]
fn an_image_past_the_ceiling_does_not_panic() {
    let Some(gpu) = require_gpu() else { return };
    let cap = gpu.max_texture_dimension();
    let wide = cap + 808;
    // A red strip; 8 px tall so the allocation stays small.
    let mut px = Vec::with_capacity(wide as usize * 8 * 4);
    for _ in 0..(wide * 8) {
        px.extend_from_slice(&Color::srgb8(0xd0, 0x20, 0x20, 0xff).to_srgb8());
    }
    let mut dl = DisplayList::new();
    dl.images
        .push(lumen_render::RgbaImage::from_raw(wide, 8, px));
    dl.push(DrawCmd::Image {
        id: ImageId(0),
        src_rect: Rect::new(0.0, 0.0, wide as f64, 8.0),
        dst_rect: Rect::new(20.0, 60.0, 180.0, 90.0),
        quality: Filter::Bilinear,
    });
    let img = gpu.render(&dl, W, H, bg());
    assert_eq!((img.width(), img.height()), (W, H));
    // It drew: the destination band is red, not background.
    let i = ((75 * W + 100) * 4) as usize;
    let p = img.pixels();
    assert!(
        p[i] > 120 && p[i + 1] < 110,
        "expected the downscaled strip to paint red at (100,75), got \
         [{}, {}, {}]",
        p[i],
        p[i + 1],
        p[i + 2]
    );
}

/// A large asset drawn into a small box must still look right after being
/// resampled to its destination footprint. Averaging, not point-sampling: a
/// checkerboard point-sampled to a small box collapses to one colour, while an
/// averaged one goes grey.
#[test]
fn a_large_asset_downscales_to_its_destination() {
    let Some(gpu) = require_gpu() else { return };
    const SRC: u32 = 2000;
    let mut px = Vec::with_capacity(SRC as usize * SRC as usize * 4);
    for y in 0..SRC {
        for x in 0..SRC {
            let on = (x / 2 + y / 2) % 2 == 0;
            let c = if on {
                Color::srgb8(0xff, 0xff, 0xff, 0xff)
            } else {
                Color::srgb8(0x00, 0x00, 0x00, 0xff)
            };
            px.extend_from_slice(&c.to_srgb8());
        }
    }
    let mut dl = DisplayList::new();
    dl.images
        .push(lumen_render::RgbaImage::from_raw(SRC, SRC, px));
    dl.push(DrawCmd::Image {
        id: ImageId(0),
        src_rect: Rect::new(0.0, 0.0, SRC as f64, SRC as f64),
        dst_rect: Rect::new(40.0, 30.0, 140.0, 130.0),
        quality: Filter::Bilinear,
    });
    gpu.take_uploaded_texels();
    let img = gpu.render(&dl, W, H, bg());
    let uploaded = gpu.take_uploaded_texels();

    // The observable. Pixel checks cannot see this: bilinear minification of a
    // full-size upload lands on roughly the same grey, so the frame looks the
    // same whether four million texels crossed the bus or forty thousand.
    assert!(
        uploaded < 200_000,
        "a {SRC}x{SRC} asset drawn into a 100x100 box uploaded {uploaded} texels; \
         expected it to be resampled to its destination footprint first \
         (full size would be {})",
        SRC as u64 * SRC as u64
    );
    let i = ((80 * W + 90) * 4) as usize;
    let p = img.pixels();
    assert!(
        (60..=200).contains(&p[i]),
        "the downscale must still look right: expected mid-grey, got {}",
        p[i]
    );
}
