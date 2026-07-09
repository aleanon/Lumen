//! Frame comparison (T.3, 05 §4): perceptual tolerance (ΔE Oklab) and visual
//! diff images, promoted from the R0 test harness so `lumen-test` goldens and
//! the GPU-parity suite share one implementation.

use crate::RgbaImage;
use lumen_core::Color;

/// A cross-backend comparison tolerance: a perceptual per-pixel ceiling plus a
/// cap on the fraction of pixels allowed to exceed it.
#[derive(Clone, Copy, Debug)]
pub struct Tolerance {
    /// Max allowed per-pixel ΔE (Oklab; unscaled Euclidean distance, range
    /// ~0–1.5, JND ≈ 0.02).
    pub max_delta_e: f32,
    /// Max allowed fraction of pixels exceeding `max_delta_e` (e.g. AA edges).
    pub max_frac_over: f64,
}

impl Tolerance {
    /// Parity for **edge-free** content (axis-aligned solid rects, nearest
    /// images): at most 0.5% of pixels may exceed ΔE 0.04.
    pub const PARITY: Tolerance = Tolerance {
        max_delta_e: 0.04,
        max_frac_over: 0.005,
    };

    /// Parity for **anti-aliased** content (rounded corners, paths,
    /// gradients): CPU (analytic coverage) and GPU (SDF/shader AA) differ
    /// along the ~1px edge seam, so a larger share of pixels may exceed the
    /// ceiling. The ceiling itself stays tight, so a *wrong*
    /// color/shape/position (which moves many interior pixels) still fails.
    pub const AA: Tolerance = Tolerance {
        max_delta_e: 0.04,
        max_frac_over: 0.04,
    };
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
///
/// # Panics
/// If the frames' dimensions differ.
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

/// Whether two frames match within `tol` — "at most `max_frac_over` of
/// pixels exceed `max_delta_e`". Returns the exceeding fraction alongside.
pub fn frames_close(a: &RgbaImage, b: &RgbaImage, tol: Tolerance) -> (bool, f64) {
    let total = (a.width() * a.height()) as usize;
    if total == 0 {
        return (true, 0.0);
    }
    let over = count_over(a, b, tol.max_delta_e);
    let frac = over as f64 / total as f64;
    (frac <= tol.max_frac_over, frac)
}

/// A visual diff for humans and agents: differing pixels solid red over a
/// dimmed grayscale of `a` — `Read` it next to the golden and the `.actual`.
///
/// # Panics
/// If the frames' dimensions differ.
pub fn diff_image(a: &RgbaImage, b: &RgbaImage) -> RgbaImage {
    assert_eq!(
        (a.width(), a.height()),
        (b.width(), b.height()),
        "diff_image requires equal dimensions"
    );
    let mut out = Vec::with_capacity(a.pixels().len());
    for (pa, pb) in a.pixels().chunks_exact(4).zip(b.pixels().chunks_exact(4)) {
        if pa != pb {
            out.extend_from_slice(&[0xff, 0x20, 0x20, 0xff]);
        } else {
            // Dimmed luma of the matching pixel keeps the layout readable.
            let l = ((pa[0] as u16 * 30 + pa[1] as u16 * 59 + pa[2] as u16 * 11) / 100 / 3) as u8;
            out.extend_from_slice(&[l, l, l, 0xff]);
        }
    }
    RgbaImage::from_raw(a.width(), a.height(), out)
}
