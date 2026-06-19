//! A1 (GPU): the runtime can render through the GPU backend via the pluggable
//! Renderer trait. GPU-gated (`--ignored`); no-op if no adapter is available.

use lumen_core::geometry::Size;
use lumen_widgets::{theme, App, BuildCx};

#[test]
#[ignore = "needs a GPU adapter (run with --ignored on a GPU runner)"]
fn runtime_renders_on_gpu_backend() {
    let Some(gpu) = lumen_render::gpu::GpuRenderer::new() else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let mut a = App::new(|_cx: &mut BuildCx| {
        theme::center_screen(theme::panel_centered(theme::display("GPU")))
    })
    .run_headless(Size::new(220.0, 140.0));
    a.pump();

    a.set_renderer(Box::new(gpu)); // swap CPU -> GPU at runtime
    assert_eq!(a.renderer_name(), "gpu");

    let img = a.screenshot();
    assert_eq!((img.width(), img.height()), (220, 140), "rendered at size");
    let painted = img
        .pixels()
        .chunks_exact(4)
        .any(|p| p[0] < 240 || p[1] < 240 || p[2] < 240);
    assert!(painted, "GPU backend produced a non-blank frame");
}
