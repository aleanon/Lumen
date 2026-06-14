//! Geometry primitives.
//!
//! Per 02 §1 these are re-exported from [`kurbo`] rather than redefined, so the
//! whole framework shares one set of point/size/rect/affine types. Lengths are
//! logical pixels (the `.lss` `px` unit) in window coordinates unless stated.

#[doc(no_inline)]
pub use kurbo::{Affine, Insets, Point, Rect, Size, Vec2};
