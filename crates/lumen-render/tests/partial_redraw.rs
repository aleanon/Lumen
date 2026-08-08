//! R6.3: a scissored partial redraw must be **byte-identical** to a full one.
//!
//! Partial redraw is the one optimisation in this renderer whose failure mode is
//! invisible in the ordinary tests: it produces correct pixels for most damage
//! rectangles and wrong ones for particular shapes. Every other GPU test here
//! renders a single frame and checks pixels, and would pass against a subtly
//! wrong implementation — so this file exists before the mechanism is trusted,
//! not after.
//!
//! Each case renders frame A, then frame B two ways from the same starting
//! state: a full redraw, and a `Load` + scissor redraw with a damage rect. The
//! two must match **exactly**. Tolerance would defeat the purpose — the failures
//! this is built to catch (a stale row at a rect edge, an off-by-one on the
//! scissor, a missed invalidation) are small, exact pixel differences.
#![cfg(feature = "wgpu")]

mod common;

use common::*;
use kurbo::Rect;
use lumen_core::Color;
use lumen_render::display_list::*;
use lumen_render::gpu::Wgpu;
use lumen_render::RgbaImage;

const W: u32 = 160;
const H: u32 = 120;

/// A background band plus one movable square — so the damage between two frames
/// is a known rectangle, and the parts that must survive are non-trivial.
fn scene(square_x: f64, square_y: f64) -> DisplayList {
    let mut dl = DisplayList::new();
    dl.push(DrawCmd::Rect {
        rect: Rect::new(0.0, 0.0, W as f64, 40.0),
        brush: Brush::Solid(Color::srgb8(0x20, 0x60, 0xc0, 0xff)),
        radii: CornerRadii::all(0.0),
        border: None,
    });
    dl.push(DrawCmd::Rect {
        rect: Rect::new(10.0, 60.0, 150.0, 100.0),
        brush: Brush::Solid(Color::srgb8(0xe0, 0xe0, 0xe0, 0xff)),
        radii: CornerRadii::all(8.0),
        border: None,
    });
    dl.push(DrawCmd::Rect {
        rect: Rect::new(square_x, square_y, square_x + 24.0, square_y + 24.0),
        brush: Brush::Solid(Color::srgb8(0xd0, 0x20, 0x20, 0xff)),
        radii: CornerRadii::all(4.0),
        border: None,
    });
    dl
}

/// The rect that bounds the difference between the two square positions, grown
/// by a pixel so anti-aliased edges are inside it. A real caller gets this from
/// `damage_between`; here it is computed explicitly so the test controls it.
fn damage(ax: f64, ay: f64, bx: f64, by: f64) -> Rect {
    Rect::new(
        ax.min(bx) - 1.0,
        ay.min(by) - 1.0,
        ax.max(bx) + 25.0,
        ay.max(by) + 25.0,
    )
}

fn render_full(gpu: &Wgpu, dl: &DisplayList) -> RgbaImage {
    gpu.render_at_scale(dl, W, H, 1.0, bg())
}

/// Frame A, then B with damage — the sequence a live app would run.
fn render_partial(gpu: &Wgpu, a: &DisplayList, b: &DisplayList, d: Rect) -> RgbaImage {
    gpu.render_at_scale(a, W, H, 1.0, bg());
    gpu.render_at_scale_dirty(b, W, H, 1.0, bg(), Some(d))
}

fn case(name: &str, ax: f64, ay: f64, bx: f64, by: f64) {
    let Some(gpu) = require_gpu_or_skip() else {
        return;
    };
    let (a, b) = (scene(ax, ay), scene(bx, by));

    // A fresh renderer for the full reference, so no retained state leaks in.
    let Some(ref_gpu) = Wgpu::new() else { return };
    let full = render_full(&ref_gpu, &b);
    let partial = render_partial(&gpu, &a, &b, damage(ax, ay, bx, by));

    let d = frame_diff(&full, &partial);
    assert_eq!(
        full.pixels(),
        partial.pixels(),
        "{name}: partial redraw differs from a full redraw — {} px differ, max \
         ΔE {:.4}. Partial redraw must be exact; any difference is a stale or \
         missing pixel, not a rounding artefact.",
        d.differing,
        d.max_delta_e
    );
}

#[test]
fn partial_matches_full_for_a_small_move() {
    case("small move", 40.0, 70.0, 52.0, 70.0);
}

/// The square crosses the rounded panel's edge, so the damage rect straddles
/// anti-aliased geometry that the scissor must not clip mid-pixel.
#[test]
fn partial_matches_full_across_an_antialiased_edge() {
    case("straddles the panel edge", 130.0, 88.0, 142.0, 92.0);
}

/// Damage touching the frame boundary: the clamp to `width`/`height` is an
/// off-by-one waiting to happen, and a scissor rect that runs past the
/// attachment is a validation error rather than a wrong pixel.
#[test]
fn partial_matches_full_at_the_frame_edge() {
    case("frame edge", 0.0, 0.0, 136.0, 96.0);
}

/// Sub-pixel motion: the damage rect gets floored/ceiled, so this is where a
/// rect that is a hair too small shows up as a stale AA fringe.
#[test]
fn partial_matches_full_for_subpixel_motion() {
    case("subpixel", 40.25, 70.75, 40.75, 70.25);
}

/// No change at all. The damage is degenerate, and the frame must still be
/// correct rather than empty — a zero-area rect means "nothing usable", which
/// has to fall back to a full redraw.
#[test]
fn partial_matches_full_for_zero_damage() {
    let Some(gpu) = require_gpu_or_skip() else {
        return;
    };
    let a = scene(40.0, 70.0);
    let Some(ref_gpu) = Wgpu::new() else { return };
    let full = render_full(&ref_gpu, &a);
    gpu.render_at_scale(&a, W, H, 1.0, bg());
    let partial = gpu.render_at_scale_dirty(&a, W, H, 1.0, bg(), Some(Rect::ZERO));
    assert_eq!(
        full.pixels(),
        partial.pixels(),
        "a zero-area damage rect must fall back to a full redraw, not draw nothing"
    );
}

/// A size change invalidates the retained root. Without that check the reused
/// target is the wrong dimensions and the frame is garbage.
#[test]
fn a_resize_invalidates_the_retained_root() {
    let Some(gpu) = require_gpu_or_skip() else {
        return;
    };
    let a = scene(40.0, 70.0);
    gpu.render_at_scale(&a, W, H, 1.0, bg());
    // Same list, different size, with damage — must not reuse the W×H target.
    let grown = gpu.render_at_scale_dirty(
        &a,
        W * 2,
        H,
        1.0,
        bg(),
        Some(damage(40.0, 70.0, 40.0, 70.0)),
    );
    let Some(ref_gpu) = Wgpu::new() else { return };
    let full = ref_gpu.render_at_scale(&a, W * 2, H, 1.0, bg());
    assert_eq!(
        full.pixels(),
        grown.pixels(),
        "a damaged frame at a new size must fall back to a full redraw"
    );
}

fn require_gpu_or_skip() -> Option<Wgpu> {
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

/// A `BackdropFilter` disqualifies the frame from partial redraw: it samples the
/// parent's resolved content mid-pass, so a partially-updated parent would feed
/// it stale pixels, and it splits the root into several passes that the
/// single-pass scissor does not cover.
///
/// The check runs on the *uncalled* list deliberately — culling to the damage
/// region could drop the backdrop command and make an ineligible frame look
/// eligible.
#[test]
fn a_backdrop_forces_a_full_redraw() {
    let Some(gpu) = require_gpu_or_skip() else {
        return;
    };
    let with_backdrop = |x: f64| {
        let mut dl = scene(x, 70.0);
        dl.push(DrawCmd::BackdropFilter {
            rect: Rect::new(20.0, 20.0, 90.0, 60.0),
            radii: CornerRadii::all(6.0),
            blur: 4.0,
            saturate: 1.2,
            refraction: 0.0,
            specular: 0.0,
        });
        dl
    };
    let (a, b) = (with_backdrop(40.0), with_backdrop(52.0));

    gpu.render_at_scale(&a, W, H, 1.0, bg());
    let partial =
        gpu.render_at_scale_dirty(&b, W, H, 1.0, bg(), Some(damage(40.0, 70.0, 52.0, 70.0)));

    let Some(ref_gpu) = Wgpu::new() else { return };
    let full = ref_gpu.render_at_scale(&b, W, H, 1.0, bg());
    assert_eq!(
        full.pixels(),
        partial.pixels(),
        "a frame containing a BackdropFilter must fall back to a full redraw"
    );
}
