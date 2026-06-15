//! Lumen — the public facade crate.
//!
//! User code and examples depend only on `lumen` (and `lumen-test`); nothing
//! imports the internal crates directly (02 §11). This crate re-exports the
//! stable public API.
#![warn(missing_docs)]

#[doc(inline)]
pub use lumen_core::{geometry, Color, Diagnostic, NodeIndex, Severity, SourceSpan, StableId};

/// Diagnostic codes (stable API, ADR-019).
pub use lumen_core::codes;

/// Reactive signals and the state store (02 §4).
pub use lumen_core::state;

/// Events and input (02 §6).
pub use lumen_core::events;

/// The semantic tree, selectors, and JSON export (03 §1–§2).
pub use lumen_core::semantics;

/// The application and headless runtime (02 §8).
#[doc(inline)]
pub use lumen_widgets::{app::FrameStats, App, BuildCx, Element, Handler, Headless};

/// Layout styling (the typed mirror of the `.lss` layout properties, 04 §3).
pub use lumen_layout as layout;

/// The display list and CPU renderer (02 §7).
pub use lumen_render as render;

/// Text shaping and layout (ADR-005).
pub use lumen_text as text;
