//! Shared gradient sampling — used by both the CPU reference renderer (conic
//! fills) and the GPU backend (ramp-texture baking, R1.4).
//!
//! Stops interpolate in Oklab (ADR-017). Baking a 1-D ramp from
//! [`sample_stops_oklab`] lets the GPU reproduce the CPU's perceptual ramp
//! exactly — only the per-pixel parameter `t` (the linear/radial/conic spatial
//! mapping) is computed in the shader.

use crate::display_list::GradientStop;
use lumen_core::Color;

/// Sample a stop list at `t` in `[0, 1]`, interpolating in Oklab.
pub fn sample_stops_oklab(stops: &[GradientStop], t: f32) -> Color {
    if stops.is_empty() {
        return Color::BLACK;
    }
    if t <= stops[0].offset {
        return stops[0].color;
    }
    for pair in stops.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if t <= b.offset {
            let span = (b.offset - a.offset).max(f32::EPSILON);
            let f = (t - a.offset) / span;
            return a.color.lerp_oklab(b.color, f);
        }
    }
    stops[stops.len() - 1].color
}

/// Bake `n` sRGB8 RGBA texels (`t` linear across `[0, 1]`) for a ramp texture.
pub fn bake_ramp(stops: &[GradientStop], n: u32) -> Vec<u8> {
    let n = n.max(2);
    let mut out = Vec::with_capacity((n * 4) as usize);
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        out.extend_from_slice(&sample_stops_oklab(stops, t).to_srgb8());
    }
    out
}
