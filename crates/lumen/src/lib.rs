//! Lumen — the public facade crate.
//!
//! User code and examples depend only on `lumen` (and `lumen-test`); nothing
//! imports `lumen_core` directly (02 §11). This crate re-exports the stable
//! public API as each subsystem lands.
#![warn(missing_docs)]

#[doc(inline)]
pub use lumen_core::{geometry, Color, Diagnostic, Severity, SourceSpan, StableId};

/// Diagnostic codes (stable API, ADR-019).
pub use lumen_core::codes;
