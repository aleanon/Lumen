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
pub mod asset;
pub mod audit;
pub mod boundary;
pub mod design;
pub mod element;
pub mod forms;
pub mod i18n;
pub mod markdown;
pub mod motion;
pub mod nav;
pub mod system;
pub mod theme;
pub mod typed;
pub mod undo;
pub mod wcag;
// ShaderWidget needs the wgpu GPU backend (CPU fallback included), which is not
// built on wasm; on the web, shaders are a WebGPU presenter concern.
#[cfg(not(target_arch = "wasm32"))]
pub mod shader;
pub mod tasks;
pub mod widgets;
pub mod widgets_extra;
pub mod widgets_m1;
pub mod widgets_m3;
pub mod widgets_m4;

pub use app::{center, App, AppSnapshot, FrameStats, Headless, ReloadResult};
/// Re-exported so downstream crates can bound on the renderer backend (e.g.
/// `Headless<R>` consumers like `lumen-agent`) without depending on `lumen-render`.
pub use lumen_render::{CpuRenderer, Renderer};
/// The data layer: executors + the `Sink` background work pushes results through.
pub use lumen_core::tasks::{InlineSpawner, ManualSpawner, Sink, Spawner};
pub use tasks::{Resource, TaskError};
pub use element::{BuildCx, Element, Handler, LeafWidget, NodeContent};
pub use typed::{Button, Checkbox, Image, Slider, Text, TextField};
