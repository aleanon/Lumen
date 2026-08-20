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

/// MOD2: the layout engine seam.
///
/// Lumen's layout is Taffy behind a wrapper (ADR-004), and no taffy type
/// escapes this crate — the hard half of substitutability was already done.
/// This trait is the missing half: the exact surface the runtime consumes, so
/// an alternative engine can be supplied without forking.
///
/// The surface is deliberately *narrow* — seven methods, derived from what
/// `app.rs` actually calls rather than from everything `LayoutTree` exposes.
/// A trait that mirrored the whole inherent API would force an implementor to
/// reproduce methods the runtime never uses (`relayout_subtree` is the clear
/// case: it exists, and wiring it in is documented as a layout-corruption bug).
///
/// Note `set_style` is included even though the runtime does not call it today.
/// It is the operation a retained arena needs, and leaving it out would bake
/// today's rebuild-everything strategy into the seam. CP5 declined the retained
/// arena at 4.46%, not permanently.
pub trait LayoutEngine {
    /// Construct an engine sized for `capacity` nodes.
    ///
    /// R3: the caller knows the previous frame's node count, and a tree built
    /// from empty pays repeated reallocation to reach it. Defaulted so an
    /// engine with no notion of capacity is unaffected.
    fn with_capacity(capacity: usize) -> Self
    where
        Self: Default + Sized,
    {
        let _ = capacity;
        Self::default()
    }

    /// Create a childless node.
    fn leaf(&mut self, style: &LayoutStyle) -> LayoutNode;

    /// Create a node with the given children.
    fn container(&mut self, style: &LayoutStyle, children: &[LayoutNode]) -> LayoutNode;

    /// Replace a node's style, marking it and its ancestors dirty.
    fn set_style(&mut self, node: LayoutNode, style: &LayoutStyle);

    /// Solve the tree rooted at `root` for `available`, filling absolute bounds.
    fn compute(&mut self, root: LayoutNode, available: kurbo::Size);

    /// Absolute window-space bounds of a node, valid after [`compute`](Self::compute).
    fn bounds(&self, node: LayoutNode) -> kurbo::Rect;

    /// Mirror the computed tree horizontally for right-to-left layout.
    fn mirror_rtl(&mut self, root: LayoutNode);

    /// Nodes whose bounds changed in the last `compute` — the damage input.
    fn touched(&self) -> usize;
}

impl LayoutEngine for LayoutTree {
    fn with_capacity(capacity: usize) -> Self {
        LayoutTree::with_capacity(capacity)
    }
    fn leaf(&mut self, style: &LayoutStyle) -> LayoutNode {
        LayoutTree::leaf_ref(self, style)
    }
    fn container(&mut self, style: &LayoutStyle, children: &[LayoutNode]) -> LayoutNode {
        LayoutTree::container_ref(self, style, children)
    }
    fn set_style(&mut self, node: LayoutNode, style: &LayoutStyle) {
        LayoutTree::set_style(self, node, style.clone());
    }
    fn compute(&mut self, root: LayoutNode, available: kurbo::Size) {
        LayoutTree::compute(self, root, available)
    }
    fn bounds(&self, node: LayoutNode) -> kurbo::Rect {
        LayoutTree::bounds(self, node)
    }
    fn mirror_rtl(&mut self, root: LayoutNode) {
        LayoutTree::mirror_rtl(self, root)
    }
    fn touched(&self) -> usize {
        LayoutTree::touched(self)
    }
}
