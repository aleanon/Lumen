//! The incremental layout tree (a thin wrapper over Taffy).
//!
//! Produces **absolute** window-space bounds (Taffy reports parent-relative
//! layout, so the wrapper accumulates offsets in a post-order walk) and supports
//! relaying out a single dirty subtree without touching the rest of the tree.

use crate::style::LayoutStyle;
use kurbo::{Point, Rect, Size};
// R1/R3: `abs` is keyed on taffy `NodeId`s this crate mints itself and is
// rebuilt every frame — std's SipHash is the wrong trade here for the same
// reason it was in lumen-text.
use lumen_core::fxhash::HashMap;
use taffy::{AvailableSpace, NodeId, Size as TSize, TaffyTree};

/// An opaque handle to a layout node (hides taffy's `NodeId`, ADR-004).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LayoutNode(NodeId);

impl LayoutNode {
    /// Mint a handle from a raw index.
    ///
    /// MOD2: a third-party [`LayoutEngine`](crate::LayoutEngine) has to be able
    /// to hand back node handles, and could not — the inner type was private
    /// and unconstructible, so the trait was an interface only Lumen could
    /// implement. Found by writing an outside-the-crate engine and discovering
    /// it did not compile.
    ///
    /// `u64` deliberately, not taffy's `NodeId`: ADR-004 keeps taffy types out
    /// of the public API, and an integer handle carries no engine semantics.
    /// The value means whatever the engine that minted it decides — Lumen never
    /// interprets it, only passes it back.
    pub fn from_raw(raw: u64) -> LayoutNode {
        LayoutNode(NodeId::from(raw))
    }

    /// The raw index behind this handle, for an engine to index its own store.
    pub fn raw(self) -> u64 {
        self.0.into()
    }
}

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
        LayoutTree::with_capacity(0)
    }

    /// An empty layout tree sized for `capacity` nodes.
    ///
    /// R3: the tree is rebuilt from empty every frame, so without a hint
    /// taffy's slotmap grows 0 → N by doubling and memmoves its contents at
    /// each step. That was **7.9%** of a 3000-row frame — `leaf_ref` →
    /// `try_insert_with_key` → `RawVec::grow_one` → `realloc` — and it is pure
    /// churn, since the previous frame already knows how many nodes there
    /// were. See `docs/profile-vs-iced-2026-08-19.md`.
    pub fn with_capacity(capacity: usize) -> LayoutTree {
        LayoutTree {
            taffy: TaffyTree::with_capacity(capacity),
            abs: HashMap::with_capacity_and_hasher(capacity, Default::default()),
            last_count: 0,
        }
    }

    /// Create a childless node.
    pub fn leaf(&mut self, style: LayoutStyle) -> LayoutNode {
        self.leaf_ref(&style)
    }

    /// [`leaf`](Self::leaf) without taking ownership.
    ///
    /// CP2.2: `copy_node` holds a `LayoutStyle` it must also retain in
    /// `node_layout_style`, so with an owning API it had to clone — 256 bytes
    /// per copied node, on the memo-hit path this campaign exists to make
    /// cheap. `to_taffy` already borrows, so ownership was never needed.
    pub fn leaf_ref(&mut self, style: &LayoutStyle) -> LayoutNode {
        LayoutNode(self.taffy.new_leaf(style.to_taffy()).expect("new_leaf"))
    }

    /// Create a node with the given children.
    pub fn container(&mut self, style: LayoutStyle, children: &[LayoutNode]) -> LayoutNode {
        self.container_ref(&style, children)
    }

    /// [`container`](Self::container) without taking ownership (see
    /// [`leaf_ref`](Self::leaf_ref)).
    pub fn container_ref(&mut self, style: &LayoutStyle, children: &[LayoutNode]) -> LayoutNode {
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

    /// Mirror the computed layout horizontally for right-to-left locales (T5.3).
    /// Each node's x is flipped within its parent's box, so `start`-aligned
    /// content moves to the right and rows read right-to-left, while sizes and
    /// vertical layout are unchanged. Call after [`LayoutTree::compute`].
    pub fn mirror_rtl(&mut self, root: LayoutNode) {
        let r = self.abs.get(&root.0).copied().unwrap_or(Rect::ZERO);
        self.mirror_node(root.0, r);
    }

    fn mirror_node(&mut self, node: NodeId, parent: Rect) {
        let b = self.abs.get(&node).copied().unwrap_or(Rect::ZERO);
        let new_x0 = parent.x0 + (parent.width() - (b.x0 - parent.x0) - b.width());
        let mirrored =
            Rect::from_origin_size(Point::new(new_x0, b.y0), Size::new(b.width(), b.height()));
        self.abs.insert(node, mirrored);
        let children = self.taffy.children(node).expect("children");
        for child in children {
            self.mirror_node(child, mirrored);
        }
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
