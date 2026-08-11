//! A direct present distinguishes "skipped this frame" from "no surface".
//!
//! It used to return `bool`, and the shell read every `false` as the permanent
//! signal — so one frame dropped during a resize drag tore down the direct path
//! and built a *second* wgpu surface on a window that was still being dragged.
//! `Surface::configure` reports a stale window as `InvalidSurface`, and wgpu 22
//! routes that through `handle_error_fatal`: the process aborts rather than
//! returning an error. A single skipped frame killed the app.
//!
//! What can be asserted without a window is the part that was silently wrong at
//! the app layer: **no frame yet is a skip, not a dead surface.** The
//! transient-acquire mapping needs a live swapchain going stale mid-drag and is
//! verified by resizing a real window (see the commit).
#![cfg(all(feature = "wgpu", not(target_arch = "wasm32")))]

use lumen_core::geometry::Size;
use lumen_render::gpu::Wgpu;
use lumen_widgets::{widgets, App, BuildCx, Element, Present};

fn build(_cx: &mut BuildCx) -> Element {
    widgets::column(vec![widgets::text("hello")]).id("root")
}

const SIZE: Size = Size {
    width: 200.0,
    height: 100.0,
};

/// The CPU renderer of record has no swapchain and never will — permanent, so
/// the shell is right to stop asking.
#[test]
fn the_cpu_renderer_reports_no_surface_as_unavailable() {
    let mut h = App::new(build).run_headless(SIZE);
    h.pump();
    assert_eq!(h.present_to_surface(), Present::Unavailable);
}

/// A GPU backend with nothing attached says the same thing. This is the arm the
/// shell's fallback is *supposed* to fire on.
#[test]
fn a_gpu_backend_with_no_surface_attached_is_unavailable() {
    let Some(gpu) = Wgpu::new() else {
        eprintln!("present_outcome: no wgpu adapter; skipping");
        return;
    };
    let mut h = App::new(build)
        .with_renderer(Box::new(gpu))
        .run_headless(SIZE);
    h.pump();
    assert_eq!(
        h.present_to_surface(),
        Present::Unavailable,
        "nothing is attached, so there is no surface to skip a frame on"
    );
}

/// Asking twice must not change the answer: the outcome is a property of the
/// surface, not a latch that degrades on repetition.
#[test]
fn the_outcome_is_stable_across_repeated_calls() {
    let mut h = App::new(build).run_headless(SIZE);
    h.pump();
    let first = h.present_to_surface();
    for _ in 0..5 {
        h.pump();
        assert_eq!(h.present_to_surface(), first);
    }
}
