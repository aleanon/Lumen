//! `lumen-widgets` — the element model, the headless app runtime, and (from
//! T0.10) the built-in widget library.
//!
//! The headless runtime ([`App`]/[`Headless`]) is the integration point that
//! ties lumen-core (tree/state/events/semantics), lumen-layout, lumen-render,
//! and lumen-text together (02 §8). It lives here, not in lumen-core, because it
//! depends on those higher crates; the `lumen` facade re-exports it as
//! `lumen::App`.
#![warn(missing_docs)]

pub mod a11y;
pub mod app;
pub mod audit;
pub mod boundary;
pub mod element;
pub mod forms;
pub mod i18n;
pub mod nav;
pub mod system;
pub mod undo;
// ShaderWidget needs the wgpu GPU backend (CPU fallback included), which is not
// built on wasm; on the web, shaders are a WebGPU presenter concern.
#[cfg(not(target_arch = "wasm32"))]
pub mod shader;
pub mod widgets;
pub mod widgets_extra;
pub mod widgets_m1;
pub mod widgets_m3;
pub mod widgets_m4;

pub use app::{center, App, AppSnapshot, FrameStats, Headless, ReloadResult};
pub use element::{BuildCx, Element, Handler};
