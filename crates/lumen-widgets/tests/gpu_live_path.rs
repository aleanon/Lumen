//! R1.1 — the live shell rasterizes through the dynamic-renderer seam
//! (`App::with_renderer(Box<dyn Renderer>)`), selecting the GPU backend at
//! startup. This test drives that exact construction headlessly and sanity-checks
//! the GPU frame on a real screen (rounded button + text): non-blank and broadly
//! close to the CPU reference (a loose bound — the GPU blends in linear vs the
//! CPU's gamma, so AA/text edges differ by design; the screen is mostly opaque,
//! so the divergent fraction stays small). Self-skips when no wgpu adapter.

use lumen_core::geometry::Size;
use lumen_core::Color;
use lumen_render::gpu::Wgpu;
use lumen_render::Renderer;
use lumen_widgets::{App, BuildCx, Button, Container, Element, Label};

fn build(cx: &mut BuildCx) -> Element {
    let rt = cx.runtime();
    let n = cx.signal("n", || 7i32);
    Container::new(vec![
        Label::new("Live GPU path").into(),
        Label::new(format!("value: {}", n.get(rt))).into(),
        Button::new("a rounded button").into(),
    ])
    .gap(10.0)
    .padding(20.0)
    .into()
}

const SIZE: Size = Size {
    width: 320.0,
    height: 180.0,
};

#[test]
fn boxed_gpu_renderer_matches_cpu_on_a_real_screen() {
    let Some(gpu) = Wgpu::new() else {
        eprintln!("gpu_live_path: no wgpu adapter; skipping");
        return;
    };

    // CPU reference (the default).
    let cpu_frame = App::new(build).run_headless(SIZE).screenshot();

    // The shell's exact path: erase the renderer to the dynamic seam and plug in
    // the GPU backend.
    let gpu_renderer: Box<dyn Renderer> = Box::new(gpu);
    let gpu_frame = App::new(build)
        .with_renderer(gpu_renderer)
        .run_headless(SIZE)
        .screenshot();

    assert_eq!(
        (gpu_frame.width(), gpu_frame.height()),
        (cpu_frame.width(), cpu_frame.height()),
        "frame size"
    );

    // The GPU must actually render content (not a blank white frame).
    let white = Color::srgb8(255, 255, 255, 255).to_srgb8();
    let painted = gpu_frame
        .pixels()
        .chunks_exact(4)
        .filter(|p| p[..] != white[..])
        .count();
    assert!(painted > 500, "GPU frame looks blank ({painted} non-bg px)");

    // ...and match the CPU reference everywhere but the AA seams (rounded button
    // corners). Text is rasterized to sprites the GPU blits 1:1, so the bulk is
    // exact; only a small fraction may exceed the perceptual ceiling.
    let total = (cpu_frame.width() * cpu_frame.height()) as usize;
    let over = cpu_frame
        .pixels()
        .chunks_exact(4)
        .zip(gpu_frame.pixels().chunks_exact(4))
        .filter(|(a, b)| {
            Color::srgb8(a[0], a[1], a[2], a[3]).delta_e_oklab(Color::srgb8(b[0], b[1], b[2], b[3]))
                > 0.04
        })
        .count();
    let frac = over as f64 / total as f64;
    assert!(
        frac < 0.05,
        "GPU↔CPU diverged on {:.2}% of pixels (budget 5%)",
        frac * 100.0
    );
}
