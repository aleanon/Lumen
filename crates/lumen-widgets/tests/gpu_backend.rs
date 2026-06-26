//! A1 (renderer generics): the runtime is generic over `R: Renderer`, chosen at
//! construction via `App::with_renderer`. The GPU backend is one such `R`
//! (GPU-gated, `--ignored`); a `Box<dyn Renderer>` is another — the dynamic
//! escape hatch, exercised here without a GPU.
#![cfg(feature = "wgpu")]

use lumen_core::geometry::Size;
use lumen_widgets::{theme, App, BuildCx, Renderer, TinySkia};

#[test]
#[ignore = "needs a GPU adapter (run with --ignored on a GPU runner)"]
fn runtime_renders_on_gpu_backend() {
    let Some(gpu) = lumen_render::gpu::Wgpu::new() else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    // Backend chosen at construction (no runtime swap): App<Wgpu>.
    let mut a = App::new(|_cx: &mut BuildCx| {
        theme::center_screen(theme::panel_centered(theme::display("GPU")))
    })
    .with_renderer(gpu)
    .run_headless(Size::new(220.0, 140.0));
    a.pump();

    assert_eq!(a.renderer_name(), "gpu");
    let img = a.screenshot();
    assert_eq!((img.width(), img.height()), (220, 140), "rendered at size");
    let painted = img
        .pixels()
        .chunks_exact(4)
        .any(|p| p[0] < 240 || p[1] < 240 || p[2] < 240);
    assert!(painted, "GPU backend produced a non-blank frame");
}

#[test]
fn boxed_renderer_opt_in_compiles_and_runs() {
    // The dynamic-dispatch opt-in: instantiate the runtime with
    // `R = Box<dyn Renderer>` (one vtable hop, the consumer's choice). Proves the
    // blanket `impl Renderer for Box<R>` + generic `App`/`Headless` line up.
    let boxed: Box<dyn Renderer> = Box::new(TinySkia);
    let mut a = App::new(|_cx: &mut BuildCx| {
        theme::center_screen(theme::panel_centered(theme::display("Boxed")))
    })
    .with_renderer(boxed)
    .run_headless(Size::new(200.0, 120.0));
    a.pump();

    assert_eq!(a.renderer_name(), "cpu", "name forwards through the box");
    let img = a.screenshot();
    assert_eq!((img.width(), img.height()), (200, 120));
    assert!(
        img.pixels()
            .chunks_exact(4)
            .any(|p| p[0] < 240 || p[1] < 240 || p[2] < 240),
        "boxed CPU backend produced a non-blank frame"
    );
}

#[test]
fn default_runtime_is_cpu() {
    let mut a =
        App::new(|_cx: &mut BuildCx| theme::display("Default")).run_headless(Size::new(80.0, 40.0));
    a.pump();
    assert_eq!(a.renderer_name(), "cpu");
}
