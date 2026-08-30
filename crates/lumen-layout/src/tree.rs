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
    abs: HashMap<NodeId, AbsEntry>,
    /// MUT4: bumped once per `compute`; entries written this compute carry it.
    /// An entry with an older stamp was pruned — its whole subtree unchanged.
    stamp: u64,
    /// RTL mirroring rewrites rounded rects after the fact, which the pruning
    /// invariant cannot see — so the first `mirror_rtl` disables pruning.
    mirrored: bool,
    /// MUT4: nodes whose style changed since the last compute, plus their
    /// ancestor chains (taffy's own `mark_dirty` shape, early-out included).
    /// The pruner must descend through these even when a rect is unchanged —
    /// a fixed-size box can keep its rect while its interior reflows.
    dirty_up: std::collections::HashSet<NodeId>,
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
            taffy: {
                // MUT4: taffy's own rounding pass walks the ENTIRE tree on
                // every `compute_layout`, dirty or not — measured at ~1.7 ms
                // of a 1.85 ms "solve" at N=50 000 while the actual warm
                // solve was 170 µs. `update_abs` applies the identical
                // formula (round the cumulative unrounded absolute, so the
                // per-node rounded values telescope to the same rects taffy
                // produced) and prunes unchanged subtrees while it is there.
                let mut t = TaffyTree::with_capacity(capacity);
                t.disable_rounding();
                t
            },
            abs: HashMap::with_capacity_and_hasher(capacity, Default::default()),
            stamp: 0,
            mirrored: false,
            dirty_up: std::collections::HashSet::default(),
            last_count: 0,
        }
    }

    /// Drop every node but keep the allocations, ready for the next frame.
    ///
    /// R6: the tree is per-frame scratch — the solved bounds are copied into
    /// the node arena and the tree itself is discarded. Recreating it each
    /// frame meant allocating and then freeing the whole slotmap, which showed
    /// as `drop_in_place<LayoutTree>` plus a share of the ~12% the frame spent
    /// in malloc/free. `taffy::TaffyTree::clear` empties the node, child and
    /// parent stores without releasing their capacity, so the next frame
    /// starts warm.
    pub fn clear(&mut self) {
        self.taffy.clear();
        self.abs.clear();
        self.last_count = 0;
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

    /// Free a node, detaching it from its parent.
    ///
    /// F2.1: with the tree retained across frames, the nodes of spans that
    /// were *re-lowered* (rather than copied forward) are orphaned and must be
    /// released, or the tree grows by the size of the changed span every
    /// frame. Removing a node does not remove its children — the runtime frees
    /// each stale node individually, which is what its `prev_meta` leftovers
    /// enumerate.
    ///
    /// A node id that is no longer in the tree is ignored rather than
    /// panicking: double-free is a bookkeeping bug, not a reason to take the
    /// app down, and `debug_assert`s on the node count catch it in tests.
    pub fn remove(&mut self, node: LayoutNode) {
        if self.taffy.style(node.0).is_ok() {
            let _ = self.taffy.remove(node.0);
        }
        self.abs.remove(&node.0);
    }

    /// Live node count — the runtime debug-asserts this against its own arena
    /// to catch a leak in the F2.1 reuse path.
    pub fn node_count(&self) -> usize {
        self.taffy.total_node_count()
    }

    /// Replace a node's style and mark it (and its ancestors) dirty.
    pub fn set_style(&mut self, node: LayoutNode, style: LayoutStyle) {
        self.taffy
            .set_style(node.0, style.to_taffy())
            .expect("set_style");
        // MUT4: the pruner may not stop anywhere above this node, even where
        // a rect is unchanged — the restyle can reflow an interior without
        // moving the box (a fixed-size panel). Same early-out as taffy's
        // upward invalidation.
        let mut cur = Some(node.0);
        while let Some(n) = cur {
            if !self.dirty_up.insert(n) {
                break;
            }
            cur = self.taffy.parent(n);
        }
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
        self.stamp = self.stamp.wrapping_add(1);
        self.last_count = self.update_abs(root.0, 0.0, 0.0, 0.0, 0.0);
        self.dirty_up.clear();
    }

    /// Recompute layout for `node`'s subtree only, within its established box.
    /// Nodes outside the subtree keep their bounds. [`LayoutTree::touched`]
    /// returns how many nodes were recomputed.
    pub fn relayout_subtree(&mut self, node: LayoutNode) {
        let cur = self
            .abs
            .get(&node.0)
            .map(|e| e.raw)
            .unwrap_or([0.0, 0.0, 0.0, 0.0]);
        self.taffy
            .compute_layout(
                node.0,
                TSize {
                    width: AvailableSpace::Definite(cur[2]),
                    height: AvailableSpace::Definite(cur[3]),
                },
            )
            .expect("compute_layout");
        self.stamp = self.stamp.wrapping_add(1);
        let origin = self.abs.get(&node.0).map(|e| (e.rect.x0, e.rect.y0));
        let (px, py) = origin.unwrap_or((0.0, 0.0));
        self.last_count = self.update_abs(node.0, cur[0], cur[1], px, py);
        self.dirty_up.clear();
    }

    /// Absolute window-space bounds of `node` (the single source of truth shared
    /// with the SoA `bounds` and `ui.getLayout`, 02 §5).
    pub fn bounds(&self, node: LayoutNode) -> Rect {
        self.abs.get(&node.0).map(|e| e.rect).unwrap_or(Rect::ZERO)
    }

    /// MUT4: whether `node` was touched by the most recent
    /// `compute`/`relayout_subtree`. `false` means the pruner proved its whole
    /// subtree unchanged — every stored bound below it is still exact.
    pub fn node_is_fresh(&self, node: LayoutNode) -> bool {
        self.abs
            .get(&node.0)
            .is_none_or(|e| e.stamp == self.stamp)
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
        // Mirroring edits rounded rects behind the pruner's back; from here on
        // every compute rewrites everything (RTL apps forgo the pruning).
        self.mirrored = true;
        let r = self.bounds(root);
        self.mirror_node(root.0, r);
    }

    fn mirror_node(&mut self, node: NodeId, parent: Rect) {
        let b = self.abs.get(&node).map(|e| e.rect).unwrap_or(Rect::ZERO);
        let new_x0 = parent.x0 + (parent.width() - (b.x0 - parent.x0) - b.width());
        let mirrored =
            Rect::from_origin_size(Point::new(new_x0, b.y0), Size::new(b.width(), b.height()));
        if let Some(e) = self.abs.get_mut(&node) {
            e.rect = mirrored;
        }
        let children = self.taffy.children(node).expect("children");
        for child in children {
            self.mirror_node(child, mirrored);
        }
    }

    /// Post-order-free recursive accumulation of absolute bounds; returns the
    /// number of nodes visited.
    /// Absolute positions from the *unrounded* solve, rounded here with
    /// taffy's own `round_layout` formula, byte-for-byte: the rounded origin
    /// is the running sum of per-node `round(location)` (taffy rounds the
    /// RELATIVE location — summing `round(cumulative)` instead drifts a pixel
    /// whenever fractions accumulate, which five doc-shot goldens caught),
    /// and the size is `round(cum + size) − round(cum)` against the
    /// *unrounded* f32 cumulative, exactly as taffy computes it. Same rects
    /// as the old rounding-on pipeline, without taffy's O(whole tree)
    /// rounding pass.
    ///
    /// MUT4 pruning: a node whose unrounded absolute rect is unchanged has an
    /// unchanged interior — its subtree was either cache-hit (spliced spans
    /// keep their taffy nodes and nothing dirties inside one) or re-solved
    /// deterministically against the same box. A freshly minted node can
    /// never match: its slotmap key is new, so `abs` has no entry. Comparing
    /// *unrounded* values also closes the subpixel hole a rounded compare
    /// has (a parent moved by 0.4 px can keep its rounded rect while a
    /// child's rounded position shifts a pixel). Skipped entries keep their
    /// old `stamp`, which is what `node_is_fresh` reports to the arena walk.
    fn update_abs(&mut self, node: NodeId, cx: f32, cy: f32, px: f64, py: f64) -> usize {
        let layout = *self.taffy.layout(node).expect("layout");
        let rx = cx + layout.location.x;
        let ry = cy + layout.location.y;
        let raw = [rx, ry, layout.size.width, layout.size.height];
        if !self.mirrored && !self.dirty_up.contains(&node) {
            if let Some(e) = self.abs.get(&node) {
                if e.raw == raw {
                    return 0; // prune: the whole subtree is current
                }
            }
        }
        let x0 = px + layout.location.x.round() as f64;
        let y0 = py + layout.location.y.round() as f64;
        let w = (rx + layout.size.width).round() - rx.round();
        let h = (ry + layout.size.height).round() - ry.round();
        let rect = Rect::new(x0, y0, x0 + w as f64, y0 + h as f64);
        self.abs.insert(
            node,
            AbsEntry {
                raw,
                rect,
                stamp: self.stamp,
            },
        );
        let children = self.taffy.children(node).expect("children");
        let mut count = 1;
        for child in children {
            count += self.update_abs(child, rx, ry, x0, y0);
        }
        count
    }
}

/// MUT4: one node's solved geometry — the unrounded absolute rect (the
/// pruning key), the rounded rect every consumer reads, and the compute
/// stamp that wrote it.
#[derive(Clone, Copy)]
struct AbsEntry {
    raw: [f32; 4],
    rect: Rect,
    stamp: u64,
}
