//! `lumen-core` — the small, stable heart of Lumen.
//!
//! In M0 this crate grows to own the node tree + SoA hot data, signals and the
//! state store, events, and the semantic tree. T0.1 seeds the pieces every
//! other crate needs from day one: geometry, [`Color`], author [`StableId`]
//! identity, and the structured [`Diagnostic`] type with its stable code
//! registry (see `diagnostics.md`).
//!
//! Nothing here is re-exported to users directly; the `lumen` facade crate is
//! the public surface (02 §11).
#![warn(missing_docs)]

pub mod color;
pub mod diagnostics;
pub mod events;
pub mod geometry;
pub mod identity;
pub mod semantics;
pub mod state;
// The SoA hot-data tree is an advanced/internal surface (02 §5): public so the
// integration layer can drive it, but hidden from docs.
#[doc(hidden)]
pub mod tree;

pub use color::Color;
pub use diagnostics::{codes, Diagnostic, Severity, SourceSpan};
pub use identity::{NodeIndex, StableId};
pub use state::{Runtime, Signal};
