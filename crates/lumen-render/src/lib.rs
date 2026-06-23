//! `lumen-render` — the display list and its CPU/GPU backends.
//!
//! M0 lands the backend-independent display list ([`DrawCmd`]) and the CPU
//! reference renderer ([`cpu::render`], tiny-skia, ADR-002). The GPU backend
//! (wgpu) and damage-aware presentation arrive in T0.11.
//!
//! ## Regression harness (rendering & performance plan, R0)
//!
//! The CPU renderer is the deterministic golden reference; the GPU backend and
//! the damage/incremental path are measured against it by a shared harness in
//! `tests/common`:
//! - **`cpu_vs_gpu`** — the GPU must match CPU within a perceptual ΔE budget
//!   (`Tolerance::PARITY`: unscaled Oklab ΔE ≤ 0.04 on ≥ 99.5% of pixels) for
//!   every command class it claims to support. It self-skips (logging, never
//!   silently passing) when no wgpu adapter is present, so it runs on a GPU box
//!   / GPU-CI and no-ops on headless CI.
//! - **`damage_equivalence`** — `cpu::render_damage(dl, dirty)` must be
//!   byte-identical to a full render cropped to `dirty`; the invariant R2 keeps.
//! - **`diff_harness`** — self-tests proving the comparators detect divergence.
#![warn(missing_docs)]

pub mod analysis;
pub mod canvas;
pub mod cpu;
pub mod display_list;
// The GPU backend (wgpu) is unavailable on wasm; the web shell renders on the CPU.
#[cfg(not(target_arch = "wasm32"))]
pub mod gpu;
pub mod image;
pub mod media;
pub mod scene;
pub mod svg;

pub use display_list::{
    BlendMode, Border, Brush, CornerRadii, DisplayList, DrawCmd, FillOrStroke, Filter, GlyphRunId,
    GradientStop, ImageId, RoundedRect, ShaderId, SpreadMode, UniformBlock,
};
pub use image::RgbaImage;

pub use analysis::{
    analyze_contrast, apca_lc, resolve_backdrop, ContrastLevel, ContrastReport, TargetContrast,
    TextTarget,
};

use lumen_core::Color;

/// A frame renderer: rasterizes a (logical-px) display list to a physical-px
/// frame. The runtime is generic over this so backends are *pluggable* — the
/// tiny-skia CPU reference renderer ([`CpuRenderer`]) is the default and the
/// golden contract; a GPU backend (and future ones, e.g. a Vello-class compute
/// rasterizer) implement the same trait and are selected at runtime rather than
/// swapped in by hand (A1).
pub trait Renderer {
    /// Rasterize `list` at `width`×`height` *physical* px, scaling logical
    /// coordinates by `scale` (HiDPI), over an opaque `background`.
    fn render_frame(
        &mut self,
        list: &DisplayList,
        width: u32,
        height: u32,
        scale: f64,
        background: Color,
    ) -> RgbaImage;

    /// A short, stable backend name (for diagnostics / the agent).
    fn name(&self) -> &'static str;
}

/// The deterministic CPU reference renderer (tiny-skia, ADR-002) — the default
/// backend and the golden contract.
#[derive(Default)]
pub struct CpuRenderer;

impl Renderer for CpuRenderer {
    fn render_frame(
        &mut self,
        list: &DisplayList,
        width: u32,
        height: u32,
        scale: f64,
        background: Color,
    ) -> RgbaImage {
        cpu::render_scaled(list, width, height, scale, background)
    }

    fn name(&self) -> &'static str {
        "cpu"
    }
}

/// A boxed renderer is itself a renderer — the dynamic-dispatch escape hatch. The
/// runtime is generic over `R: Renderer` (zero-cost by default); a consumer who
/// wants a backend chosen at runtime instantiates the runtime with
/// `R = Box<dyn Renderer>` and pays one vtable hop, by their own choice.
impl<R: Renderer + ?Sized> Renderer for Box<R> {
    fn render_frame(
        &mut self,
        list: &DisplayList,
        width: u32,
        height: u32,
        scale: f64,
        background: Color,
    ) -> RgbaImage {
        (**self).render_frame(list, width, height, scale, background)
    }

    fn name(&self) -> &'static str {
        (**self).name()
    }
}
