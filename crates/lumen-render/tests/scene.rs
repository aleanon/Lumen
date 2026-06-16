//! T6.1: multi-threaded culling matches the serial pass and the backend seam.
use kurbo::Rect;
use lumen_render::scene::{cull_visible, Backend};

fn serial(bounds: &[Rect], vp: Rect) -> Vec<usize> {
    bounds
        .iter()
        .enumerate()
        .filter(|(_, b)| b.x0 < vp.x1 && b.x1 > vp.x0 && b.y0 < vp.y1 && b.y1 > vp.y0)
        .map(|(i, _)| i)
        .collect()
}

fn scene(n: usize) -> Vec<Rect> {
    // Deterministic pseudo-random rects spread over a large canvas.
    (0..n)
        .map(|i| {
            let x = ((i * 2654435761) % 100_000) as f64;
            let y = ((i * 40503) % 100_000) as f64;
            Rect::new(x, y, x + 30.0, y + 20.0)
        })
        .collect()
}

#[test]
fn parallel_cull_matches_serial() {
    let vp = Rect::new(10_000.0, 10_000.0, 20_000.0, 20_000.0);
    for n in [100usize, 9_000, 50_000] {
        let b = scene(n);
        assert_eq!(cull_visible(&b, vp), serial(&b, vp), "n={n}");
    }
}

#[test]
fn backend_seam_is_distinct() {
    assert_ne!(Backend::Cpu, Backend::Gpu);
    assert_ne!(Backend::Gpu, Backend::VelloCompute);
}
