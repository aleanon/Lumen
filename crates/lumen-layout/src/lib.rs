//! `lumen-layout` — incremental layout over Taffy, behind a wrapper (ADR-004).
//!
//! The engine is replaceable: nothing outside this crate sees a taffy type.
//! [`LayoutStyle`] mirrors the layout property set of 04 §3; [`LayoutTree`]
//! computes absolute window-space bounds and supports dirty-subtree relayout.
#![warn(missing_docs)]

pub mod style;
pub mod tree;

pub use style::{
    Align, Dim, Display, Edges, FlexDirection, FlexWrap, GridLine, GridTrack, LayoutStyle, Position,
};
pub use tree::{LayoutNode, LayoutTree};
