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

pub mod binding;
pub mod color;
pub mod diagnostics;
pub mod events;
pub mod geometry;
pub mod gesture;
pub mod identity;
/// `#[state_registry]` runtime support (02 §4): stored trait objects.
#[cfg(feature = "snapshot")]
pub mod registry;
pub mod semantics;

/// Text decoration lines (PROP1 `text-decoration`). A first-party enum for the
/// same reason as [`CursorShape`]: the style engine names it, the paint layer
/// consumes it, and neither should name the other's types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextDecoration {
    /// No line.
    #[default]
    None,
    /// A line along the baseline.
    Underline,
    /// A line through the middle of the x-height.
    LineThrough,
}

/// Pointer cursor shapes (PROP1 `cursor`). A first-party enum: the style
/// engine and runtime must not name winit, and the shell maps this to whatever
/// its platform calls the same shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CursorShape {
    /// The platform default arrow.
    #[default]
    Default,
    /// A pointing hand — interactive affordance.
    Pointer,
    /// Text/I-beam.
    Text,
    /// Busy.
    Wait,
    /// Crosshair.
    Crosshair,
    /// Move/grab.
    Move,
    /// The action is not allowed here.
    NotAllowed,
    /// Explicitly hidden.
    None,
}

pub mod state;
pub mod tasks;
// The SoA hot-data tree is an advanced/internal surface (02 §5): public so the
// integration layer can drive it, but hidden from docs.
#[doc(hidden)]
pub mod tree;

pub use binding::{Dynamic, Prop};
pub use color::Color;
pub use diagnostics::{codes, Diagnostic, Severity, SourceSpan};
pub use identity::{NodeIndex, StableId};
pub use state::{Runtime, Signal};
#[cfg(not(target_arch = "wasm32"))]
pub use tasks::ThreadPoolSpawner;
pub use tasks::{InlineSpawner, ManualSpawner, Sink, Spawner};
