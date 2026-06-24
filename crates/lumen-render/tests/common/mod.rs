//! Shared rendering test harness — Phase **R0** of the rendering & performance
//! plan (`docs/plan-rendering-performance.md`).
//!
//! Three reusable pieces, consumed by `diff_harness.rs`, `cpu_vs_gpu.rs`, and
//! `damage_equivalence.rs` (and later by R1–R4):
//!
//! - [`corpus`] — named display lists, one per [`DrawCmd`] class plus composite
//!   scenes, each tagged with the GPU [`Cap`]ability it exercises.
//! - [`frame_diff`] / [`assert_frames_close`] — tolerance-based comparison
//!   (perceptual ΔE in Oklab, 05 §4) for cross-backend parity.
//! - [`assert_frames_exact`] — byte-identical comparison, the contract the
//!   damage / incremental path (R2) must keep.
//!
//! ## CPU parity vs. the GPU's linear blend
//!
//! The GPU composites in **linear light** (sRGB target) while the `CpuRenderer`
//! reference blends in **gamma**, so they agree *exactly* only on opaque, non-AA,
//! nearest-sampled content ([`exact_vs_cpu`]) and intentionally diverge on
//! blended / anti-aliased / bilinear scenes (the GPU is the "nicer" linear result
//! the live agent sees). [`gpu_supported`] still records which command classes
//! the GPU *renders* (all of them bar `Shader`, which has no producer);
//! `cpu_vs_gpu` asserts tight parity for the exact subset and treats the rest as
//! informational (logged ΔE) while checking the GPU renders + is deterministic.
#![allow(dead_code)] // each test binary uses only part of this shared module

use kurbo::{BezPath, Point, Rect};
use lumen_core::Color;
use lumen_render::display_list::*;
use lumen_render::RgbaImage;

/// Frame width used across the harness (matches `gpu_parity`'s scale).
pub const W: u32 = 200;
/// Frame height used across the harness.
pub const H: u32 = 150;

/// Opaque white background.
pub fn bg() -> Color {
    Color::srgb8(255, 255, 255, 255)
}

/// A `W`×`H` frame filled with the background — for "did the GPU draw anything?"
/// checks.
pub fn blank_frame() -> RgbaImage {
    let px = bg().to_srgb8();
    let mut buf = Vec::with_capacity((W * H * 4) as usize);
    for _ in 0..(W * H) {
        buf.extend_from_slice(&px);
    }
    RgbaImage::from_raw(W, H, buf)
}

// --- capabilities -----------------------------------------------------------

/// A renderer capability a corpus scene exercises. Granularity matches the R1
/// sub-phases that deliver each one on the GPU.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cap {
    /// Opaque, square-cornered, solid-color rects (live today).
    RectSolid,
    /// Nearest-sampled image blits (live today).
    Image,
    /// Rounded corners and/or borders (R1.2).
    RectRounded,
    /// Filled/stroked arbitrary paths (R1.3, `lyon`).
    Path,
    /// Linear/radial/conic gradients (R1.4).
    Gradient,
    /// Layers: clip / opacity / blend (R1.5).
    Layer,
    /// Custom shader fills (R1, later).
    Shader,
    /// Glass `backdrop-filter` (blur + saturate within a rounded region).
    Backdrop,
}

/// The command classes the **GPU backend renders** (all bar `Shader`, which has
/// no producer). This is no longer a pixel-parity claim — see [`exact_vs_cpu`]
/// for which scenes match the CPU exactly under linear blending.
pub fn gpu_supported(cap: Cap) -> bool {
    matches!(
        cap,
        Cap::RectSolid
            | Cap::Image
            | Cap::RectRounded
            | Cap::Path
            | Cap::Gradient
            | Cap::Layer
            | Cap::Backdrop
    )
}

/// A named scene plus the capability it exercises.
pub struct Scene {
    /// Stable name (used in failure messages and as a golden key).
    pub name: &'static str,
    /// The capability this scene primarily exercises.
    pub cap: Cap,
    /// The display list to render.
    pub dl: DisplayList,
}

// --- tolerance + diff -------------------------------------------------------

/// A cross-backend comparison tolerance: a perceptual per-pixel ceiling plus a
/// cap on the fraction of pixels allowed to exceed it.
#[derive(Clone, Copy, Debug)]
pub struct Tolerance {
    /// Max allowed per-pixel ΔE (Oklab).
    pub max_delta_e: f32,
    /// Max allowed fraction of pixels exceeding `max_delta_e` (e.g. AA edges).
    pub max_frac_over: f64,
}

impl Tolerance {
    /// Parity for **edge-free** content (axis-aligned solid rects, nearest
    /// images): `delta_e_oklab` is *unscaled* Euclidean Oklab distance (range
    /// ~0–1.5, JND ≈ 0.02), so the ceiling is tight — at most 0.5% of pixels may
    /// exceed ΔE 0.04.
    pub const PARITY: Tolerance = Tolerance {
        max_delta_e: 0.04,
        max_frac_over: 0.005,
    };

    /// Parity for **anti-aliased** content (rounded corners, paths, gradients):
    /// CPU (tiny-skia analytic coverage) and GPU (SDF/shader AA) differ along
    /// the ~1px edge seam, so a larger share of pixels may exceed the ceiling.
    /// The ceiling itself stays tight, so a *wrong* color/shape/position (which
    /// moves many interior pixels) still fails.
    pub const AA: Tolerance = Tolerance {
        max_delta_e: 0.04,
        max_frac_over: 0.04,
    };
}

/// Whether a scene's GPU output must match the CPU reference *exactly* (within
/// `PARITY`). The GPU blends in **linear** light (sRGB target) while the CPU
/// reference blends in **gamma**, so they agree only on opaque, non-AA,
/// nearest-sampled content; anti-aliased / blended / bilinear scenes diverge by
/// design (the GPU is the "nicer" linear result the live agent sees). For those
/// the differential is informational, not a parity assertion.
pub fn exact_vs_cpu(name: &str) -> bool {
    matches!(name, "rect_solid" | "image_checker" | "mixed_order")
}

/// The parity tolerance appropriate for a capability's content.
pub fn tolerance(cap: Cap) -> Tolerance {
    match cap {
        Cap::RectSolid | Cap::Image => Tolerance::PARITY,
        // The blur is a 3-box approximation; CPU uses u8 intermediates and the
        // GPU keeps more precision, so allow a broader perceptual margin.
        Cap::Backdrop => Tolerance {
            max_delta_e: 0.04,
            max_frac_over: 0.10,
        },
        _ => Tolerance::AA,
    }
}

/// A per-pixel comparison report over two equal-sized frames.
#[derive(Clone, Copy, Debug)]
pub struct DiffReport {
    /// Total pixels compared.
    pub total: usize,
    /// Pixels differing in any channel (byte-exact sense).
    pub differing: usize,
    /// Largest single-channel absolute difference (0–255).
    pub max_channel_delta: u8,
    /// Largest per-pixel ΔE (Oklab).
    pub max_delta_e: f32,
}

/// Compare two equal-sized frames pixel by pixel.
pub fn frame_diff(a: &RgbaImage, b: &RgbaImage) -> DiffReport {
    assert_eq!(
        (a.width(), a.height()),
        (b.width(), b.height()),
        "frame_diff requires equal dimensions"
    );
    let mut r = DiffReport {
        total: (a.width() * a.height()) as usize,
        differing: 0,
        max_channel_delta: 0,
        max_delta_e: 0.0,
    };
    for (pa, pb) in a.pixels().chunks_exact(4).zip(b.pixels().chunks_exact(4)) {
        let mut any = false;
        for k in 0..4 {
            let d = pa[k].abs_diff(pb[k]);
            if d != 0 {
                any = true;
            }
            r.max_channel_delta = r.max_channel_delta.max(d);
        }
        if any {
            r.differing += 1;
        }
        let de = Color::srgb8(pa[0], pa[1], pa[2], pa[3])
            .delta_e_oklab(Color::srgb8(pb[0], pb[1], pb[2], pb[3]));
        r.max_delta_e = r.max_delta_e.max(de);
    }
    r
}

/// Count pixels whose ΔE (Oklab) exceeds `ceiling`.
pub fn count_over(a: &RgbaImage, b: &RgbaImage, ceiling: f32) -> usize {
    a.pixels()
        .chunks_exact(4)
        .zip(b.pixels().chunks_exact(4))
        .filter(|(pa, pb)| {
            Color::srgb8(pa[0], pa[1], pa[2], pa[3])
                .delta_e_oklab(Color::srgb8(pb[0], pb[1], pb[2], pb[3]))
                > ceiling
        })
        .count()
}

/// Assert two frames match within `tol` (perceptual). The check is "at most
/// `max_frac_over` of pixels exceed `max_delta_e`" — a real divergence on a
/// meaningful share of pixels fails; a thin AA seam passes. Panics with a
/// detailed report otherwise.
pub fn assert_frames_close(a: &RgbaImage, b: &RgbaImage, tol: Tolerance, ctx: &str) {
    let report = frame_diff(a, b);
    let over = count_over(a, b, tol.max_delta_e);
    let frac = if report.total == 0 {
        0.0
    } else {
        over as f64 / report.total as f64
    };
    assert!(
        frac <= tol.max_frac_over,
        "{ctx}: parity failed — {over}/{} px exceed ΔE {} ({:.4}%, budget {:.4}%), max ΔE {:.4}",
        report.total,
        tol.max_delta_e,
        frac * 100.0,
        tol.max_frac_over * 100.0,
        report.max_delta_e,
    );
}

/// Assert two frames are byte-identical (the damage / determinism contract).
pub fn assert_frames_exact(a: &RgbaImage, b: &RgbaImage, ctx: &str) {
    assert_eq!(
        (a.width(), a.height()),
        (b.width(), b.height()),
        "{ctx}: size mismatch"
    );
    let n = a.diff_count(b);
    assert_eq!(n, 0, "{ctx}: {n} pixels differ (must be byte-identical)");
}

// --- the corpus -------------------------------------------------------------

/// All harness scenes, one (or more) per [`Cap`].
pub fn corpus() -> Vec<Scene> {
    vec![
        Scene {
            name: "rect_solid",
            cap: Cap::RectSolid,
            dl: scene_rect_solid(),
        },
        Scene {
            name: "image_checker",
            cap: Cap::Image,
            dl: scene_image(),
        },
        Scene {
            name: "image_bilinear",
            cap: Cap::Image,
            dl: scene_image_bilinear(),
        },
        Scene {
            name: "rect_rounded",
            cap: Cap::RectRounded,
            dl: scene_rect_rounded(),
        },
        Scene {
            name: "path",
            cap: Cap::Path,
            dl: scene_path(),
        },
        Scene {
            name: "gradient_linear",
            cap: Cap::Gradient,
            dl: scene_with_brush(Brush::LinearGradient {
                start: Point::new(10.0, 10.0),
                end: Point::new(190.0, 10.0),
                stops: ramp(),
                spread: SpreadMode::Pad,
            }),
        },
        Scene {
            name: "gradient_radial",
            cap: Cap::Gradient,
            dl: scene_with_brush(Brush::RadialGradient {
                center: Point::new(100.0, 75.0),
                radius: 80.0,
                stops: ramp(),
                spread: SpreadMode::Pad,
            }),
        },
        Scene {
            name: "gradient_conic",
            cap: Cap::Gradient,
            dl: scene_with_brush(Brush::ConicGradient {
                center: Point::new(100.0, 75.0),
                start_angle: 0.0,
                stops: ramp(),
            }),
        },
        // A rounded rect filled with a gradient — the fill is clipped to the
        // corner radii (R1).
        Scene {
            name: "gradient_rounded",
            cap: Cap::Gradient,
            dl: {
                let mut dl = DisplayList::new();
                dl.push(DrawCmd::Rect {
                    rect: Rect::new(20.0, 18.0, 180.0, 132.0),
                    brush: Brush::LinearGradient {
                        start: Point::new(20.0, 18.0),
                        end: Point::new(180.0, 132.0),
                        stops: ramp(),
                        spread: SpreadMode::Pad,
                    },
                    radii: CornerRadii::all(28.0),
                    border: None,
                });
                dl
            },
        },
        Scene {
            name: "layer_clip_opacity",
            cap: Cap::Layer,
            dl: scene_layer(),
        },
        Scene {
            name: "shader_fallback",
            cap: Cap::Shader,
            dl: scene_shader(),
        },
        // Mixed-Z within one layer: rect, then an image on top, then a rect on
        // top — only correct if draws follow display-list order (R1). Opaque +
        // integer-aligned, so it's tight PARITY.
        Scene {
            name: "mixed_order",
            cap: Cap::RectSolid,
            dl: scene_mixed_order(),
        },
        // Glass: a textured backdrop blurred + saturated within a rounded region.
        Scene {
            name: "backdrop_glass",
            cap: Cap::Backdrop,
            dl: scene_backdrop(),
        },
    ]
}

fn scene_backdrop() -> DisplayList {
    let mut dl = DisplayList::new();
    // A high-contrast backdrop so the blur is visible/discriminating.
    dl.push(DrawCmd::Rect {
        rect: Rect::new(0.0, 0.0, W as f64, H as f64),
        brush: Brush::Solid(Color::srgb8(0xf0, 0xf2, 0xf6, 0xff)),
        radii: CornerRadii::ZERO,
        border: None,
    });
    dl.push(DrawCmd::Rect {
        rect: Rect::new(0.0, 0.0, 100.0, H as f64),
        brush: Brush::Solid(Color::srgb8(0xe0, 0x30, 0x40, 0xff)),
        radii: CornerRadii::ZERO,
        border: None,
    });
    dl.push(DrawCmd::Rect {
        rect: Rect::new(120.0, 30.0, 190.0, 120.0),
        brush: Brush::Solid(Color::srgb8(0x20, 0x60, 0xd0, 0xff)),
        radii: CornerRadii::ZERO,
        border: None,
    });
    // Glass: blur (+slight saturate) the backdrop within a rounded region.
    dl.push(DrawCmd::BackdropFilter {
        rect: Rect::new(50.0, 40.0, 160.0, 115.0),
        radii: CornerRadii::all(18.0),
        blur: 6.0,
        saturate: 1.2,
    });
    dl
}

fn scene_mixed_order() -> DisplayList {
    let mut dl = DisplayList::new();
    dl.push(DrawCmd::Rect {
        rect: Rect::new(30.0, 30.0, 130.0, 110.0),
        brush: Brush::Solid(Color::srgb8(0xd0, 0x30, 0x30, 0xff)),
        radii: CornerRadii::ZERO,
        border: None,
    });
    // 2×2 checkerboard on top of the red rect.
    let a = Color::srgb8(40, 60, 200, 255).to_srgb8();
    let b = Color::srgb8(240, 220, 70, 255).to_srgb8();
    let mut px = Vec::new();
    for (p, q) in [(a, b), (b, a)] {
        px.extend_from_slice(&p);
        px.extend_from_slice(&q);
    }
    dl.images.push(RgbaImage::from_raw(2, 2, px));
    dl.push(DrawCmd::Image {
        id: ImageId(0),
        src_rect: Rect::new(0.0, 0.0, 2.0, 2.0),
        dst_rect: Rect::new(70.0, 60.0, 150.0, 120.0),
        quality: Filter::Nearest,
    });
    // A green rect on top of the image — must win.
    dl.push(DrawCmd::Rect {
        rect: Rect::new(110.0, 90.0, 180.0, 140.0),
        brush: Brush::Solid(Color::srgb8(0x20, 0xa0, 0x40, 0xff)),
        radii: CornerRadii::ZERO,
        border: None,
    });
    dl
}

/// Three opaque, integer-aligned, square-cornered solid rects — the GPU-parity
/// safe subset (no blending, no AA edges).
fn scene_rect_solid() -> DisplayList {
    let mut dl = DisplayList::new();
    for (r, c) in [
        (
            Rect::new(10.0, 10.0, 110.0, 90.0),
            Color::srgb8(0x1a, 0x73, 0xe8, 0xff),
        ),
        (
            Rect::new(60.0, 50.0, 180.0, 130.0),
            Color::srgb8(0x2e, 0xa0, 0x43, 0xff),
        ),
        (
            Rect::new(130.0, 20.0, 170.0, 60.0),
            Color::srgb8(0xe8, 0x1a, 0x4b, 0xff),
        ),
    ] {
        dl.push(DrawCmd::Rect {
            rect: r,
            brush: Brush::Solid(c),
            radii: CornerRadii::ZERO,
            border: None,
        });
    }
    dl
}

fn scene_image() -> DisplayList {
    // 2x2 opaque checkerboard, scaled to 32px at an integer origin (nearest).
    let r = Color::srgb8(220, 40, 40, 255).to_srgb8();
    let y = Color::srgb8(250, 210, 60, 255).to_srgb8();
    let mut px = Vec::new();
    for (a, b) in [(r, y), (y, r)] {
        px.extend_from_slice(&a);
        px.extend_from_slice(&b);
    }
    let mut dl = DisplayList::new();
    dl.images.push(RgbaImage::from_raw(2, 2, px));
    dl.push(DrawCmd::Image {
        id: ImageId(0),
        src_rect: Rect::new(0.0, 0.0, 2.0, 2.0),
        dst_rect: Rect::new(20.0, 100.0, 52.0, 132.0),
        quality: Filter::Nearest,
    });
    dl
}

fn scene_image_bilinear() -> DisplayList {
    // A 4×4 gradient-ish image scaled up ~40× with bilinear sampling.
    let mut px = Vec::new();
    for y in 0..4u32 {
        for x in 0..4u32 {
            px.extend_from_slice(
                &Color::srgb8((x * 80) as u8, (y * 80) as u8, 120, 255).to_srgb8(),
            );
        }
    }
    let mut dl = DisplayList::new();
    dl.images.push(RgbaImage::from_raw(4, 4, px));
    dl.push(DrawCmd::Image {
        id: ImageId(0),
        src_rect: Rect::new(0.0, 0.0, 4.0, 4.0),
        dst_rect: Rect::new(20.0, 15.0, 180.0, 135.0),
        quality: Filter::Bilinear,
    });
    dl
}

fn scene_rect_rounded() -> DisplayList {
    let mut dl = DisplayList::new();
    dl.push(DrawCmd::Rect {
        rect: Rect::new(20.0, 18.0, 180.0, 132.0),
        brush: Brush::Solid(Color::srgb8(0x1a, 0x73, 0xe8, 0xff)),
        radii: CornerRadii::all(22.0),
        border: Some(Border {
            width: 4.0,
            color: Color::srgb8(0x0b, 0x3d, 0x91, 0xff),
        }),
    });
    dl
}

fn scene_path() -> DisplayList {
    let mut dl = DisplayList::new();
    let mut tri = BezPath::new();
    tri.move_to((100.0, 14.0));
    tri.line_to((180.0, 134.0));
    tri.line_to((20.0, 134.0));
    tri.close_path();
    dl.push(DrawCmd::Path {
        path: tri,
        brush: Brush::Solid(Color::srgb8(0x2e, 0xa0, 0x43, 0xff)),
        style: FillOrStroke::Fill,
    });
    let mut wave = BezPath::new();
    wave.move_to((20.0, 75.0));
    wave.quad_to((70.0, 20.0), (100.0, 75.0));
    wave.quad_to((130.0, 130.0), (180.0, 75.0));
    dl.push(DrawCmd::Path {
        path: wave,
        brush: Brush::Solid(Color::srgb8(0x80, 0x10, 0x10, 0xff)),
        style: FillOrStroke::Stroke { width: 6.0 },
    });
    dl
}

fn ramp() -> Vec<GradientStop> {
    vec![
        GradientStop {
            offset: 0.0,
            color: Color::srgb8(0xff, 0x00, 0x00, 0xff),
        },
        GradientStop {
            offset: 0.5,
            color: Color::srgb8(0x00, 0xff, 0x00, 0xff),
        },
        GradientStop {
            offset: 1.0,
            color: Color::srgb8(0x00, 0x00, 0xff, 0xff),
        },
    ]
}

fn scene_with_brush(brush: Brush) -> DisplayList {
    let mut dl = DisplayList::new();
    dl.push(DrawCmd::Rect {
        rect: Rect::new(10.0, 10.0, 190.0, 140.0),
        brush,
        radii: CornerRadii::ZERO,
        border: None,
    });
    dl
}

fn scene_layer() -> DisplayList {
    let mut dl = DisplayList::new();
    dl.push(DrawCmd::Rect {
        rect: Rect::new(0.0, 0.0, W as f64, H as f64),
        brush: Brush::Solid(Color::srgb8(0xee, 0xee, 0xee, 0xff)),
        radii: CornerRadii::ZERO,
        border: None,
    });
    dl.push(DrawCmd::PushLayer {
        clip: Some(RoundedRect {
            rect: Rect::new(40.0, 30.0, 160.0, 120.0),
            radii: CornerRadii::all(20.0),
        }),
        opacity: 0.6,
        transform: kurbo::Affine::IDENTITY,
        blend: BlendMode::SourceOver,
    });
    dl.push(DrawCmd::Rect {
        rect: Rect::new(0.0, 0.0, W as f64, H as f64),
        brush: Brush::Solid(Color::srgb8(0xe8, 0x1a, 0x4b, 0xff)),
        radii: CornerRadii::ZERO,
        border: None,
    });
    dl.push(DrawCmd::PopLayer);
    dl
}

fn scene_shader() -> DisplayList {
    let mut dl = DisplayList::new();
    dl.push(DrawCmd::Shader {
        id: ShaderId(0),
        rect: Rect::new(30.0, 25.0, 170.0, 125.0),
        uniforms: UniformBlock {
            fallback: Color::srgb8(0x66, 0x33, 0x99, 0xff),
        },
    });
    dl
}
