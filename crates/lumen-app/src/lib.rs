//! `lumen-app` — the application runtime (SD1).
//!
//! The frame pipeline (build → reconcile → layout → paint → semantics), the
//! `Element` tree it consumes, the OS-service request model, and the layout
//! audits. Extracted from `lumen-widgets`, which now depends on this crate and
//! re-exports it, so existing paths keep working.
//!
//! The dependency runs one way: the runtime knows nothing about the widget
//! catalogue. The single edge that would have made the split cyclic — the focus
//! ring reaching for `theme::accent()` — was reversed, and `theme` now reads
//! [`element::accent_color`] instead.
#![warn(missing_docs)]

pub mod app;
pub mod audit;
pub mod element;
pub mod system;
/// The async/data layer (`cx.resource`, `cx.task`) — an inherent `impl BuildCx`,
/// so it can only live in the crate that defines `BuildCx`.
pub mod tasks;

/// R1: the hasher moved to `lumen-core` so `lumen-text` can share it.
/// Re-exported under the old path so this crate's call sites are unchanged.
pub(crate) use lumen_core::fxhash;

pub use app::{
    App, DefaultPlatform, Direct, DirectDyn, FrameStats, Headless, NodeWriter, PlatformConfig,
};
pub use element::{BuildCx, Element, Handler, NodeContent, Text};
