//! The incremental layout tree (a thin wrapper over Taffy).
//!
//! Produces **absolute** window-space bounds (Taffy reports parent-relative
//! layout, so the wrapper accumulates offsets in a post-order walk) and supports
//! relaying out a single dirty subtree without touching the rest of the tree.

use crate::style::LayoutStyle;
use kurbo::{Point, Rect, Size};
use std::collections::HashMap;
use taffy::{AvailableSpace, NodeId, Size as TSize, TaffyTree};

/// An opaque handle to a layout node (hides taffy's `NodeId`, ADR-004).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LayoutNode(NodeId);

/// A layout tree. Build it with [`LayoutTree::leaf`]/[`LayoutTree::container`],
/// then [`LayoutTree::compute`]; read results via [`LayoutTree::bounds`].
pub struct LayoutTree {
    taffy: TaffyTree<()>,
    abs: HashMap<NodeId, Rect>,
    last_count: usize,
}

impl Default for LayoutTree {
    fn default() -> Self {
        LayoutTree::new()
    }
}

impl LayoutTree {
    /// An empty layout tree.
    pub fn new() -> LayoutTree {
        LayoutTree {
            taffy: TaffyTree::new(),
            abs: HashMap::new(),
            last_count: 0,
        }
    }

    /// Create a childless node.
    pub fn leaf(&mut self, style: LayoutStyle) -> LayoutNode {
        LayoutNode(self.taffy.new_leaf(style.to_taffy()).expect("new_leaf"))
    }

    /// Create a node with the given children.
    pub fn container(&mut self, style: LayoutStyle, children: &[LayoutNode]) -> LayoutNode {
        let ids: Vec<NodeId> = children.iter().map(|c| c.0).collect();
        LayoutNode(
            self.taffy
                .new_with_children(style.to_taffy(), &ids)
                .expect("new_with_children"),
        )
    }

    /// Replace a node's style and mark it (and its ancestors) dirty.
    pub fn set_style(&mut self, node: LayoutNode, style: LayoutStyle) {
        self.taffy
            .set_style(node.0, style.to_taffy())
            .expect("set_style");
    }

    /// Compute layout for the whole tree rooted at `root`, filling absolute
    /// bounds. `available` is the window/viewport size.
    pub fn compute(&mut self, root: LayoutNode, available: Size) {
        self.taffy
            .compute_layout(
                root.0,
                TSize {
                    width: AvailableSpace::Definite(available.width as f32),
                    height: AvailableSpace::Definite(available.height as f32),
                },
            )
            .expect("compute_layout");
        self.last_count = self.update_abs(root.0, Point::ZERO);
    }

    /// Recompute layout for `node`'s subtree only, within its established box.
    /// Nodes outside the subtree keep their bounds. [`LayoutTree::touched`]
    /// returns how many nodes were recomputed.
    pub fn relayout_subtree(&mut self, node: LayoutNode) {
        let cur = self.abs.get(&node.0).copied().unwrap_or(Rect::ZERO);
        self.taffy
            .compute_layout(
                node.0,
                TSize {
                    width: AvailableSpace::Definite(cur.width() as f32),
                    height: AvailableSpace::Definite(cur.height() as f32),
                },
            )
            .expect("compute_layout");
        self.last_count = self.update_abs(node.0, cur.origin());
    }

    /// Absolute window-space bounds of `node` (the single source of truth shared
    /// with the SoA `bounds` and `ui.getLayout`, 02 §5).
    pub fn bounds(&self, node: LayoutNode) -> Rect {
        self.abs.get(&node.0).copied().unwrap_or(Rect::ZERO)
    }

    /// Number of nodes whose bounds were recomputed by the last
    /// `compute`/`relayout_subtree` call.
    pub fn touched(&self) -> usize {
        self.last_count
    }

    /// Post-order-free recursive accumulation of absolute bounds; returns the
    /// number of nodes visited.
    fn update_abs(&mut self, node: NodeId, parent_origin: Point) -> usize {
        let layout = *self.taffy.layout(node).expect("layout");
        let origin = Point::new(
            parent_origin.x + layout.location.x as f64,
            parent_origin.y + layout.location.y as f64,
        );
        let rect = Rect::from_origin_size(
            origin,
            Size::new(layout.size.width as f64, layout.size.height as f64),
        );
        self.abs.insert(node, rect);
        let children = self.taffy.children(node).expect("children");
        let mut count = 1;
        for child in children {
            count += self.update_abs(child, origin);
        }
        count
    }
}
