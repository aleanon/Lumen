//! Scene processing (T6.1): the renderer-backend selection seam and a
//! multi-threaded scene/culling pass over command bounds.
//!
//! The deterministic tiny-skia CPU renderer stays the renderer of record;
//! [`Backend`] selects what executes the display list at runtime. A Vello-class
//! compute-shader path rasterizer is the intended production GPU path — that
//! integration is large GPU work tracked separately (see the decision log); the
//! seam + the parallel scene build land here.

use kurbo::Rect;

/// Which rasterizer executes the display list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// The deterministic tiny-skia CPU reference renderer (renderer of record).
    Cpu,
    /// The wgpu GPU renderer (atlased rects/images/glyphs; lyon paths).
    Gpu,
    /// A Vello-class compute-shader path rasterizer (production GPU path;
    /// integration is future work).
    VelloCompute,
}

/// Below this many items, culling runs serially (threading overhead isn't worth
/// it). Above it, the work is split across scoped threads.
const PAR_THRESHOLD: usize = 8_192;

/// Cull `bounds` against `viewport`, returning the indices of items that
/// intersect it. For large scenes the work is split across threads; the result
/// is identical (same order) to the serial pass.
pub fn cull_visible(bounds: &[Rect], viewport: Rect) -> Vec<usize> {
    if bounds.len() < PAR_THRESHOLD {
        return bounds
            .iter()
            .enumerate()
            .filter(|(_, b)| intersects(b, &viewport))
            .map(|(i, _)| i)
            .collect();
    }

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let chunk = bounds.len().div_ceil(threads);

    // Each chunk yields its visible *global* indices; concatenating the chunk
    // results in order reproduces the serial order exactly.
    let parts: Vec<Vec<usize>> = std::thread::scope(|s| {
        let handles: Vec<_> = bounds
            .chunks(chunk)
            .enumerate()
            .map(|(ci, items)| {
                let base = ci * chunk;
                s.spawn(move || {
                    items
                        .iter()
                        .enumerate()
                        .filter(|(_, b)| intersects(b, &viewport))
                        .map(|(i, _)| base + i)
                        .collect::<Vec<usize>>()
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut out = Vec::new();
    for p in parts {
        out.extend(p);
    }
    out
}

/// Axis-aligned rectangle intersection (touching edges don't count).
fn intersects(a: &Rect, b: &Rect) -> bool {
    a.x0 < b.x1 && a.x1 > b.x0 && a.y0 < b.y1 && a.y1 > b.y0
}
