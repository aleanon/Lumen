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
