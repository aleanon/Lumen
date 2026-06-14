//! `lumen-render` — the display list and its CPU/GPU backends.
//!
//! M0 lands the backend-independent display list ([`DrawCmd`]) and the CPU
//! reference renderer ([`cpu::render`], tiny-skia, ADR-002). The GPU backend
//! (wgpu) and damage-aware presentation arrive in T0.11.
#![warn(missing_docs)]

pub mod cpu;
pub mod display_list;
pub mod image;

pub use display_list::{
    BlendMode, Border, Brush, CornerRadii, DisplayList, DrawCmd, FillOrStroke, Filter, GlyphRunId,
    GradientStop, ImageId, RoundedRect, ShaderId, SpreadMode, UniformBlock,
};
pub use image::RgbaImage;
