//! The node tree and its structure-of-arrays (SoA) hot data.
//!
//! Widget *logic* lives in a tree (ergonomic, hierarchical — matches how styles
//! cascade and events bubble), but the per-frame *hot data* lives in flat
//! parallel arrays keyed by [`NodeIndex`] (02 §5, ADR-008). Culling,
//! hit-testing, and damage aggregation are linear scans/walks over these arrays
//! — never widget-trait calls.
//!
//! This is crate-internal: the public contract is the widget/app surface, not
//! the array layout. Only the observable invariants are binding (02 §5):
//! - hit-test order is highest `z` first, then reverse document order, honoring
//!   `clip` and `HIT_TESTABLE`;
//! - a node's `bounds` is the single source of truth shared with semantics and
//!   `ui.getLayout`.
//!
//! Several items here have no non-test consumer until the headless `App` wires
//! the tree in (T0.9); the module-level `allow(dead_code)` below is removed then.

use crate::identity::NodeIndex;
use bitflags::bitflags;
use kurbo::{Affine, Point, Rect};

/// Sentinel for "this node has no taffy node yet" in [`Tree::lnode`].
/// `u64::MAX` rather than `0`, because a real `taffy::NodeId` can be 0.
const NO_LNODE: u64 = u64::MAX;

bitflags! {
    /// Per-node state bits stored in the SoA `flags` array (02 §5).
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct NodeFlags: u32 {
        /// Node participates in layout/paint and can be hit.
        const VISIBLE      = 1 << 0;
        /// Layout of this subtree is stale.
        const DIRTY_LAYOUT = 1 << 1;
        /// Paint of this node is stale.
        const DIRTY_PAINT  = 1 << 2;
        /// Node can receive keyboard focus.
        const FOCUSABLE    = 1 << 3;
        /// Node participates in hit-testing.
        const HIT_TESTABLE = 1 << 4;
        /// Node is disabled (no input).
        const DISABLED     = 1 << 5;
        /// Pointer is currently over the node.
        const HOVERED      = 1 << 6;
        /// Node currently holds focus.
        const FOCUSED      = 1 << 7;
        /// Node is currently pressed.
        const PRESSED      = 1 << 8;
    }
}

/// The node arena: a generational slot allocator plus the SoA hot-data arrays
/// and intrusive tree links. All arrays are indexed by `NodeIndex::index`.
pub struct Tree {
    // allocator
    generation: Vec<u32>,
    alive: Vec<bool>,
    free: Vec<u32>,
    live_count: usize,

    // SoA hot data (02 §5)
    bounds: Vec<Rect>,
    transform: Vec<Affine>,
    opacity: Vec<f32>,
    clip: Vec<Option<Rect>>,
    flags: Vec<NodeFlags>,
    z: Vec<u32>,
    /// F2.1: the taffy node this arena node was laid out with, as an opaque
    /// `u64` (`LayoutNode::raw`), or `NO_LNODE` if it has none yet.
    ///
    /// Stored here rather than in a side `HashMap<NodeIndex, LayoutNode>`
    /// because the copy-forward path needs it keyed by *previous-frame* node
    /// on every copied node — a dense slot is O(1) with no hashing, and
    /// hashing per node is exactly what R3/R4 were removing. `lumen-core`
    /// cannot name `LayoutNode` (it sits below `lumen-layout`), which is what
    /// `LayoutNode::{raw, from_raw}` exist for.
    lnode: Vec<u64>,

    // intrusive tree links (02 §5)
    parent: Vec<NodeIndex>,
    first_child: Vec<NodeIndex>,
    /// Tail of each node's child list. Without it, appending a child walks the
    /// whole sibling chain, making the build of a k-child container O(k²) — a
    /// 1000-row column cost ~500 000 pointer hops per frame and was the single
    /// largest symbol in a profile of `pump()` at 23%.
    last_child: Vec<NodeIndex>,
    next_sibling: Vec<NodeIndex>,
    /// F2.2: the sibling *before* this one, making the child list doubly
    /// linked so [`detach`](Self::detach) is O(1).
    ///
    /// Splice-in-place detaches a memo-hit span's root from its
    /// previous-frame parent on every rebuild. With a singly-linked list that
    /// costs a scan for the predecessor, which is O(1) only while spans are
    /// detached in child order — a list whose changed and unchanged rows
    /// alternate degrades to O(n²). Four bytes a node buys the worst case.
    prev_sibling: Vec<NodeIndex>,

    root: NodeIndex,
}

impl Default for Tree {
    fn default() -> Self {
        Tree::new()
    }
}

impl Tree {
    /// An empty tree with no root.
    pub fn new() -> Tree {
        Tree::with_capacity(0)
    }

    /// An empty tree with room for `capacity` nodes.
    ///
    /// R4: the arena is rebuilt from empty every frame, so without a hint its
    /// backing vectors grow by doubling and memmove at each step. The previous
    /// frame already knows how many nodes there were.
    pub fn with_capacity(capacity: usize) -> Tree {
        let c = capacity;
        Tree {
            generation: Vec::with_capacity(c),
            alive: Vec::with_capacity(c),
            free: Vec::new(),
            live_count: 0,
            bounds: Vec::with_capacity(c),
            transform: Vec::with_capacity(c),
            opacity: Vec::with_capacity(c),
            clip: Vec::with_capacity(c),
            flags: Vec::with_capacity(c),
            z: Vec::with_capacity(c),
            lnode: Vec::with_capacity(c),
            parent: Vec::with_capacity(c),
            first_child: Vec::with_capacity(c),
            last_child: Vec::with_capacity(c),
            next_sibling: Vec::with_capacity(c),
            prev_sibling: Vec::with_capacity(c),
            root: NodeIndex::NONE,
        }
    }

    /// The root node, or [`NodeIndex::NONE`] if the tree is empty.
    pub fn root(&self) -> NodeIndex {
        self.root
    }

    /// Number of live nodes.
    pub fn len(&self) -> usize {
        self.live_count
    }

    /// Whether the tree has no live nodes.
    pub fn is_empty(&self) -> bool {
        self.live_count == 0
    }

    /// Whether `n` refers to a currently-live node (generation must match).
    pub fn is_alive(&self, n: NodeIndex) -> bool {
        let i = n.index() as usize;
        n.is_some() && i < self.alive.len() && self.alive[i] && self.generation[i] == n.generation()
    }

    // --- structure mutation -------------------------------------------------

    /// Allocate the root. Panics if a root already exists.
    pub fn insert_root(&mut self) -> NodeIndex {
        assert!(self.root.is_none(), "tree already has a root");
        let n = self.alloc(NodeIndex::NONE);
        self.root = n;
        n
    }

    /// Allocate a node and append it as the last child of `parent`.
    /// `parent` must be live.
    pub fn insert_child(&mut self, parent: NodeIndex) -> NodeIndex {
        debug_assert!(self.is_alive(parent), "insert_child: dead parent");
        let n = self.alloc(parent);
        self.link_last_child(parent, n);
        n
    }

    /// Move `node` to become the last child of `new_parent`. Returns `false`
    /// (and does nothing) if the move is invalid — `node` is the root, either
    /// index is dead, or `new_parent` lies within `node`'s own subtree (which
    /// would create a cycle).
    pub fn reparent(&mut self, node: NodeIndex, new_parent: NodeIndex) -> bool {
        if !self.is_alive(node) || !self.is_alive(new_parent) || node == self.root {
            return false;
        }
        if self.is_in_subtree(new_parent, node) {
            return false;
        }
        self.unlink(node);
        self.link_last_child(new_parent, node);
        true
    }

    /// F2.2 splice primitives — allocate a node with no parent, adopt an
    /// already-live node, and free one node without touching its children.
    ///
    /// These exist because splice-in-place keeps the arena across frames: a
    /// memo-hit span's nodes are *moved* under their new parent rather than
    /// re-created, and the previous frame's spine is freed node by node
    /// afterwards. `insert_root`/`remove` cannot serve — the first asserts the
    /// tree has no root yet, the second recurses into the subtree it is
    /// supposed to leave alone.
    ///
    /// Allocate a node with no parent and no place in the tree yet.
    pub fn insert_orphan(&mut self) -> NodeIndex {
        self.alloc(NodeIndex::NONE)
    }

    /// Make `n` the tree's root. `n` must be live and parentless.
    pub fn set_root(&mut self, n: NodeIndex) {
        debug_assert!(self.is_alive(n), "set_root: dead node");
        debug_assert!(
            self.parent[n.index() as usize].is_none(),
            "set_root: node still has a parent"
        );
        self.root = n;
    }

    /// Detach `n` from its parent's child list; `n` and its subtree stay live.
    pub fn detach(&mut self, n: NodeIndex) {
        if self.is_alive(n) {
            self.unlink(n);
        }
    }

    /// Append the already-live, already-detached `child` to `parent`.
    pub fn attach_last_child(&mut self, parent: NodeIndex, child: NodeIndex) {
        debug_assert!(self.is_alive(parent), "attach_last_child: dead parent");
        debug_assert!(self.is_alive(child), "attach_last_child: dead child");
        debug_assert!(
            self.parent[child.index() as usize].is_none(),
            "attach_last_child: child is still attached — detach it first"
        );
        self.link_last_child(parent, child);
    }

    /// Free exactly one node, leaving its children alone.
    ///
    /// The caller is enumerating a known-dead set and frees every node in it,
    /// so unlinking each from its (also dead) parent would be wasted work.
    /// Reads the node's links before they are cleared, so a caller walking
    /// the doomed subtree must push a node's children *before* freeing it.
    pub fn free_one(&mut self, n: NodeIndex) {
        if self.is_alive(n) {
            if self.root == n {
                self.root = NodeIndex::NONE;
            }
            self.dealloc(n);
        }
    }

    /// The subtree rooted at `n`, in preorder.
    ///
    /// F2.2 uses this only for the AN1 animation check, which is gated on an
    /// animation actually running — a memo hit on a still frame never
    /// enumerates a span's nodes at all.
    pub fn subtree_preorder(&self, n: NodeIndex) -> Vec<NodeIndex> {
        let mut out = Vec::new();
        if self.is_alive(n) {
            self.visit_preorder(n, &mut out);
        }
        out
    }

    /// Every live node, in slot order (not document order).
    pub fn iter_live(&self) -> impl Iterator<Item = NodeIndex> + '_ {
        self.alive
            .iter()
            .enumerate()
            .filter(|(_, &a)| a)
            .map(|(i, _)| NodeIndex::new(i as u32, self.generation[i]))
    }

    /// Remove `node` and its entire subtree, recycling all their slots.
    /// Removing the root empties the tree.
    pub fn remove(&mut self, node: NodeIndex) {
        if !self.is_alive(node) {
            return;
        }
        self.unlink(node);
        if node == self.root {
            self.root = NodeIndex::NONE;
        }
        // Free the subtree in document order.
        let mut doomed = Vec::new();
        self.collect_subtree(node, &mut doomed);
        for n in doomed {
            self.dealloc(n);
        }
    }

    // --- hot-data accessors -------------------------------------------------

    /// The window-space bounds of `n` — the single source of truth shared with
    /// semantics and `ui.getLayout` (02 §5). Bounds-checked: a stale index (from
    /// a node that no longer exists after a rebuild) returns the empty rect rather
    /// than panicking, so a late event can never crash the runtime.
    pub fn bounds(&self, n: NodeIndex) -> Rect {
        self.bounds
            .get(n.index() as usize)
            .copied()
            .unwrap_or(Rect::ZERO)
    }
    pub fn set_bounds(&mut self, n: NodeIndex, r: Rect) {
        self.bounds[n.index() as usize] = r;
    }
    pub fn z(&self, n: NodeIndex) -> u32 {
        self.z.get(n.index() as usize).copied().unwrap_or(0)
    }
    pub fn set_z(&mut self, n: NodeIndex, z: u32) {
        self.z[n.index() as usize] = z;
    }
    /// The taffy node this node was laid out with (F2.1), or `None`.
    ///
    /// Opaque here: the value round-trips through
    /// `lumen_layout::LayoutNode::{raw, from_raw}` at the call site.
    /// Bounds-checked like [`z`](Self::z) — a stale index reads `None`.
    pub fn lnode(&self, n: NodeIndex) -> Option<u64> {
        self.lnode
            .get(n.index() as usize)
            .copied()
            .filter(|v| *v != NO_LNODE)
    }
    /// Record the taffy node this node was laid out with (F2.1).
    pub fn set_lnode(&mut self, n: NodeIndex, raw: u64) {
        self.lnode[n.index() as usize] = raw;
    }
    /// Bounds-checked (see [`bounds`](Self::bounds)): a stale index returns
    /// `NodeFlags::empty()`.
    pub fn flags(&self, n: NodeIndex) -> NodeFlags {
        self.flags
            .get(n.index() as usize)
            .copied()
            .unwrap_or(NodeFlags::empty())
    }
    pub fn set_flags(&mut self, n: NodeIndex, f: NodeFlags) {
        self.flags[n.index() as usize] = f;
    }
    pub fn set_clip(&mut self, n: NodeIndex, c: Option<Rect>) {
        self.clip[n.index() as usize] = c;
    }
    pub fn set_opacity(&mut self, n: NodeIndex, o: f32) {
        self.opacity[n.index() as usize] = o;
    }
    pub fn set_transform(&mut self, n: NodeIndex, t: Affine) {
        self.transform[n.index() as usize] = t;
    }

    // --- link accessors -----------------------------------------------------

    pub fn parent(&self, n: NodeIndex) -> NodeIndex {
        self.parent[n.index() as usize]
    }
    pub fn first_child(&self, n: NodeIndex) -> NodeIndex {
        self.first_child[n.index() as usize]
    }
    pub fn next_sibling(&self, n: NodeIndex) -> NodeIndex {
        self.next_sibling[n.index() as usize]
    }

    // --- iteration ----------------------------------------------------------

    /// Live nodes in document order (depth-first preorder from the root).
    pub fn document_order(&self) -> Vec<NodeIndex> {
        let mut out = Vec::with_capacity(self.live_count);
        if self.root.is_some() {
            self.visit_preorder(self.root, &mut out);
        }
        out
    }

    /// Live nodes in paint order: preorder, with **siblings** stably sorted by
    /// ascending `z` (PROP1 `z-index`).
    ///
    /// Sorting siblings rather than the flat list is what makes this safe. The
    /// paint pass tracks clip layers **by depth** and so requires a strict
    /// preorder — a parent must precede its children and a subtree must stay
    /// contiguous. [`z_order`](Self::z_order) sorts the flat document order and
    /// breaks both; reordering within a parent breaks neither.
    ///
    /// It is also the right semantics rather than a compromise: CSS scopes
    /// `z-index` to a stacking context, and the parent is the context here. A
    /// high-`z` child does not escape a low-`z` ancestor, which is what CSS does
    /// too. Equal `z` keeps document order, so a tree with no `z-index` produces
    /// exactly the previous list.
    pub fn paint_order(&self) -> Vec<NodeIndex> {
        let mut out = Vec::with_capacity(self.live_count);
        if self.root.is_some() {
            self.visit_paint_order(self.root, &mut out);
        }
        out
    }

    fn visit_paint_order(&self, node: NodeIndex, out: &mut Vec<NodeIndex>) {
        out.push(node);

        // Fast path: walk the sibling chain directly when no child sets `z`.
        //
        // `z-index` is set on approximately nothing — but the sort ran for every
        // node on every frame regardless, which made a container's paint cost
        // O(k log k) in its child count where document order had been O(k). On a
        // 6000-row column that is a 6000-element sort and a 6000-element
        // allocation per frame, to reproduce the order the chain was already in.
        let mut c = self.first_child[node.index() as usize];
        let mut any_z = false;
        while c.is_some() {
            if self.z[c.index() as usize] != 0 {
                any_z = true;
                break;
            }
            c = self.next_sibling[c.index() as usize];
        }
        if !any_z {
            let mut c = self.first_child[node.index() as usize];
            while c.is_some() {
                self.visit_paint_order(c, out);
                c = self.next_sibling[c.index() as usize];
            }
            return;
        }

        let mut kids: Vec<NodeIndex> = Vec::new();
        let mut c = self.first_child[node.index() as usize];
        while c.is_some() {
            kids.push(c);
            c = self.next_sibling[c.index() as usize];
        }
        // Stable: equal `z` keeps document order.
        kids.sort_by_key(|&k| self.z[k.index() as usize]);
        for k in kids {
            self.visit_paint_order(k, out);
        }
    }

    /// Live nodes in paint (z) order: document order stably sorted by ascending
    /// `z`. Lower `z` paints first; equal `z` keeps document order.
    pub fn z_order(&self) -> Vec<NodeIndex> {
        let mut out = self.document_order();
        out.sort_by_key(|&n| self.z[n.index() as usize]);
        out
    }

    /// Topmost hittable node at window point `p`, or `None`.
    ///
    /// Honors `VISIBLE | HIT_TESTABLE`, the effective clip (intersection of
    /// ancestor + own clip rects), and the binding order: highest `z` wins, ties
    /// broken by reverse document order (later in document order is on top).
    /// Implemented as a single preorder walk over the link arrays.
    pub fn hit_test(&self, p: Point) -> Option<NodeIndex> {
        if self.root.is_none() {
            return None;
        }
        // best = (z, preorder_pos, node); maximize lexically.
        let mut best: Option<(u32, usize, NodeIndex)> = None;
        let mut pos: usize = 0;
        self.hit_visit(self.root, None, p, &mut pos, &mut best);
        best.map(|(_, _, n)| n)
    }

    // --- internals ----------------------------------------------------------

    fn alloc(&mut self, parent: NodeIndex) -> NodeIndex {
        self.live_count += 1;
        if let Some(i) = self.free.pop() {
            let iu = i as usize;
            self.alive[iu] = true;
            self.bounds[iu] = Rect::ZERO;
            self.transform[iu] = Affine::IDENTITY;
            self.opacity[iu] = 1.0;
            self.clip[iu] = None;
            self.flags[iu] = NodeFlags::VISIBLE;
            self.z[iu] = 0;
            self.lnode[iu] = NO_LNODE;
            self.parent[iu] = parent;
            self.first_child[iu] = NodeIndex::NONE;
            self.last_child[iu] = NodeIndex::NONE;
            self.next_sibling[iu] = NodeIndex::NONE;
            self.prev_sibling[iu] = NodeIndex::NONE;
            NodeIndex::new(i, self.generation[iu])
        } else {
            let i = self.generation.len() as u32;
            self.generation.push(0);
            self.alive.push(true);
            self.bounds.push(Rect::ZERO);
            self.transform.push(Affine::IDENTITY);
            self.opacity.push(1.0);
            self.clip.push(None);
            self.flags.push(NodeFlags::VISIBLE);
            self.z.push(0);
            self.lnode.push(NO_LNODE);
            self.parent.push(parent);
            self.first_child.push(NodeIndex::NONE);
            self.last_child.push(NodeIndex::NONE);
            self.next_sibling.push(NodeIndex::NONE);
            self.prev_sibling.push(NodeIndex::NONE);
            NodeIndex::new(i, 0)
        }
    }

    fn dealloc(&mut self, n: NodeIndex) {
        let i = n.index() as usize;
        debug_assert!(self.alive[i]);
        self.alive[i] = false;
        self.generation[i] = self.generation[i].wrapping_add(1);
        self.first_child[i] = NodeIndex::NONE;
        self.next_sibling[i] = NodeIndex::NONE;
        self.prev_sibling[i] = NodeIndex::NONE;
        self.parent[i] = NodeIndex::NONE;
        self.free.push(n.index());
        self.live_count -= 1;
    }

    /// Append `child` to `parent`'s sibling list. `child` must be detached.
    fn link_last_child(&mut self, parent: NodeIndex, child: NodeIndex) {
        let pi = parent.index() as usize;
        self.parent[child.index() as usize] = parent;
        self.next_sibling[child.index() as usize] = NodeIndex::NONE;
        self.prev_sibling[child.index() as usize] = self.last_child[pi];
        match self.last_child[pi] {
            tail if tail.is_some() => self.next_sibling[tail.index() as usize] = child,
            _ => self.first_child[pi] = child,
        }
        self.last_child[pi] = child;
    }

    /// Detach `node` from its parent's child list (node itself stays alive).
    fn unlink(&mut self, node: NodeIndex) {
        let ni = node.index() as usize;
        let parent = self.parent[ni];
        if parent.is_none() {
            return;
        }
        let pi = parent.index() as usize;
        let before = self.prev_sibling[ni];
        let after = self.next_sibling[ni];
        // F2.2: O(1) — the predecessor is known rather than searched for.
        if before.is_some() {
            self.next_sibling[before.index() as usize] = after;
        } else {
            self.first_child[pi] = after;
        }
        if after.is_some() {
            self.prev_sibling[after.index() as usize] = before;
        } else {
            self.last_child[pi] = before;
        }
        self.next_sibling[ni] = NodeIndex::NONE;
        self.prev_sibling[ni] = NodeIndex::NONE;
        self.parent[ni] = NodeIndex::NONE;
    }

    fn collect_subtree(&self, node: NodeIndex, out: &mut Vec<NodeIndex>) {
        out.push(node);
        let mut c = self.first_child[node.index() as usize];
        while c.is_some() {
            self.collect_subtree(c, out);
            c = self.next_sibling[c.index() as usize];
        }
    }

    fn visit_preorder(&self, node: NodeIndex, out: &mut Vec<NodeIndex>) {
        out.push(node);
        let mut c = self.first_child[node.index() as usize];
        while c.is_some() {
            self.visit_preorder(c, out);
            c = self.next_sibling[c.index() as usize];
        }
    }

    /// Is `needle` equal to `root` or one of its descendants?
    fn is_in_subtree(&self, needle: NodeIndex, root: NodeIndex) -> bool {
        if needle == root {
            return true;
        }
        let mut c = self.first_child[root.index() as usize];
        while c.is_some() {
            if self.is_in_subtree(needle, c) {
                return true;
            }
            c = self.next_sibling[c.index() as usize];
        }
        false
    }

    fn hit_visit(
        &self,
        node: NodeIndex,
        parent_clip: Option<Rect>,
        p: Point,
        pos: &mut usize,
        best: &mut Option<(u32, usize, NodeIndex)>,
    ) {
        let i = node.index() as usize;
        let my_clip = intersect_clip(parent_clip, self.clip[i]);
        let this_pos = *pos;
        *pos += 1;

        let f = self.flags[i];
        let hittable = f.contains(NodeFlags::VISIBLE | NodeFlags::HIT_TESTABLE);
        if hittable && self.bounds[i].contains(p) && my_clip.is_none_or(|c| c.contains(p)) {
            let key = (self.z[i], this_pos);
            if best.is_none_or(|(bz, bp, _)| key > (bz, bp)) {
                *best = Some((self.z[i], this_pos, node));
            }
        }

        let mut c = self.first_child[i];
        while c.is_some() {
            self.hit_visit(c, my_clip, p, pos, best);
            c = self.next_sibling[c.index() as usize];
        }
    }
}

/// Intersect two optional clip rects. `None` means "no clip".
fn intersect_clip(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(a), Some(b)) => Some(a.intersect(b)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect::new(x, y, x + w, y + h)
    }

    /// A straightforward, independently-written reference for hit-testing: scan
    /// document order, recompute each node's effective clip by walking to the
    /// root, collect candidates, pick max by (z, position). Used as an oracle
    /// against the optimized [`Tree::hit_test`].
    fn hit_test_naive(t: &Tree, p: Point) -> Option<NodeIndex> {
        let order = t.document_order();
        let mut best: Option<(u32, usize, NodeIndex)> = None;
        for (pos, &node) in order.iter().enumerate() {
            let i = node.index() as usize;
            let f = t.flags[i];
            if !f.contains(NodeFlags::VISIBLE | NodeFlags::HIT_TESTABLE) {
                continue;
            }
            if !t.bounds[i].contains(p) {
                continue;
            }
            // effective clip: every ancestor's (and own) clip must contain p.
            let mut cur = node;
            let mut clipped = false;
            while cur.is_some() {
                if let Some(c) = t.clip[cur.index() as usize] {
                    if !c.contains(p) {
                        clipped = true;
                        break;
                    }
                }
                cur = t.parent[cur.index() as usize];
            }
            if clipped {
                continue;
            }
            let key = (t.z[i], pos);
            if best.is_none_or(|(bz, bp, _)| key > (bz, bp)) {
                best = Some((t.z[i], pos, node));
            }
        }
        best.map(|(_, _, n)| n)
    }

    /// The shape the `last_child` tail cache actually gets wrong: remove a
    /// node from the MIDDLE of a sibling list, so `unlink` takes its
    /// mid-list-removal branch rather than the head branch.
    ///
    /// Written explicitly rather than left to the proptest's saved-seed file.
    /// The seed was produced by deliberately ablating that branch while adding
    /// the invariant, so checking it in would have recorded a "failure case
    /// proptest generated in the past" for a failure that never happened —
    /// legible only via a disclaimer. A named test says what it means.
    #[test]
    fn removing_from_the_middle_keeps_last_child_pointing_at_the_real_tail() {
        let mut t = Tree::new();
        let root = t.insert_root();
        let kids: Vec<_> = (0..5).map(|_| t.insert_child(root)).collect();
        check_invariants(&t);

        // Middle removal: not the head, not the tail.
        t.remove(kids[2]);
        check_invariants(&t);
        assert_eq!(
            t.last_child[root.index() as usize],
            kids[4],
            "removing a middle child must not disturb the tail"
        );

        // Now the tail itself, which is the other branch.
        t.remove(kids[4]);
        check_invariants(&t);
        assert_eq!(
            t.last_child[root.index() as usize],
            kids[3],
            "removing the tail must move it back to the new last child"
        );

        // …down to empty.
        for k in [kids[0], kids[1], kids[3]] {
            t.remove(k);
            check_invariants(&t);
        }
        assert_eq!(
            t.last_child[root.index() as usize],
            NodeIndex::NONE,
            "an empty child list must have no tail"
        );
    }

    /// Panics if any binding invariant is violated.
    fn check_invariants(t: &Tree) {
        // allocator accounting
        let alive_slots = t.alive.iter().filter(|&&a| a).count();
        assert_eq!(
            alive_slots, t.live_count,
            "live_count disagrees with alive[]"
        );
        assert_eq!(
            t.alive.len(),
            t.free.len() + alive_slots,
            "free + alive != capacity"
        );

        // links point only at live, generation-matching nodes (or NONE)
        let check_link = |label: &str, link: NodeIndex| {
            if link.is_some() {
                assert!(t.is_alive(link), "{label} dangling link: {link:?}");
            }
        };
        for i in 0..t.alive.len() {
            if !t.alive[i] {
                continue;
            }
            check_link("parent", t.parent[i]);
            check_link("first_child", t.first_child[i]);
            check_link("next_sibling", t.next_sibling[i]);
            // `last_child` is a redundant tail CACHE over the sibling chain,
            // repaired by hand in `link_last_child` and in two separate
            // branches of `unlink`. It was omitted here when it was added, so
            // a stale tail pointing at a recycled slot went unchecked: the
            // next `link_last_child` would then write `next_sibling` onto a
            // node no longer in the parent's list, and the appended child
            // would be parented but unreachable — an element that simply
            // never appears, with no panic.
            check_link("last_child", t.last_child[i]);
            // F2.2 made the child list doubly linked so `detach` is O(1).
            // `prev_sibling` is the same kind of redundant cache `last_child`
            // is, and would fail the same silent way: a stale back-pointer
            // makes `unlink` splice the wrong node out of the list, dropping
            // a live element from the tree with no panic.
            check_link("prev_sibling", t.prev_sibling[i]);
        }

        // prev_sibling is the exact inverse of next_sibling
        for i in 0..t.alive.len() {
            if !t.alive[i] {
                continue;
            }
            let n = NodeIndex::new(i as u32, t.generation[i]);
            let next = t.next_sibling[i];
            if next.is_some() {
                assert_eq!(
                    t.prev_sibling[next.index() as usize],
                    n,
                    "next_sibling/prev_sibling disagree at {n:?}"
                );
            }
            // the head of a child list has no predecessor
            let parent = t.parent[i];
            if parent.is_some() && t.first_child[parent.index() as usize] == n {
                assert!(
                    t.prev_sibling[i].is_none(),
                    "list head {n:?} has a prev_sibling"
                );
            }
        }

        // document order reaches every live node exactly once (no cycles, no
        // orphans)
        let order = t.document_order();
        let mut seen = std::collections::HashSet::new();
        for &n in &order {
            assert!(seen.insert(n), "node visited twice (cycle): {n:?}");
        }
        assert_eq!(order.len(), t.live_count, "doc order misses live nodes");

        // parent/child symmetry
        if t.root.is_some() {
            assert!(
                t.parent[t.root.index() as usize].is_none(),
                "root has a parent"
            );
        }
        for &n in &order {
            // each child's parent is n, and n is reachable as a child of its parent
            let mut c = t.first_child[n.index() as usize];
            let mut walked_tail = NodeIndex::NONE;
            while c.is_some() {
                assert_eq!(t.parent[c.index() as usize], n, "child's parent != n");
                walked_tail = c;
                c = t.next_sibling[c.index() as usize];
            }
            // …and the cached tail must BE the tail. Liveness alone is not
            // enough: a stale-but-live `last_child` still produces a correct
            // document order, so every existing assertion here passes while
            // the very next append goes to the wrong node. This walks the
            // chain and compares, which is the only way to catch that.
            assert_eq!(
                t.last_child[n.index() as usize],
                walked_tail,
                "last_child is not the real tail of {n:?}'s sibling chain"
            );
            let p = t.parent[n.index() as usize];
            if p.is_some() {
                let mut found = false;
                let mut ch = t.first_child[p.index() as usize];
                while ch.is_some() {
                    if ch == n {
                        found = true;
                        break;
                    }
                    ch = t.next_sibling[ch.index() as usize];
                }
                assert!(found, "node {n:?} not in its parent's child list");
            }
        }
    }

    #[test]
    fn empty_tree() {
        let t = Tree::new();
        assert_eq!(t.len(), 0);
        assert!(t.root().is_none());
        assert!(t.document_order().is_empty());
        assert_eq!(t.hit_test(Point::new(1.0, 1.0)), None);
        check_invariants(&t);
    }

    #[test]
    fn generational_reuse_invalidates_stale_index() {
        let mut t = Tree::new();
        let root = t.insert_root();
        let a = t.insert_child(root);
        assert!(t.is_alive(a));
        t.remove(a);
        assert!(!t.is_alive(a), "removed node must read as dead");
        // next allocation reuses the slot with a bumped generation
        let b = t.insert_child(root);
        assert_eq!(a.index(), b.index(), "slot should be reused");
        assert_ne!(a.generation(), b.generation(), "generation must change");
        assert!(!t.is_alive(a), "stale index must not alias the new node");
        assert!(t.is_alive(b));
        check_invariants(&t);
    }

    #[test]
    fn document_order_is_preorder() {
        // root -> [a -> [c], b]
        let mut t = Tree::new();
        let root = t.insert_root();
        let a = t.insert_child(root);
        let b = t.insert_child(root);
        let c = t.insert_child(a);
        assert_eq!(t.document_order(), vec![root, a, c, b]);
        check_invariants(&t);
    }

    #[test]
    fn reparent_rejects_cycles() {
        let mut t = Tree::new();
        let root = t.insert_root();
        let a = t.insert_child(root);
        let b = t.insert_child(a);
        assert!(
            !t.reparent(a, b),
            "cannot reparent a under its own descendant"
        );
        assert!(!t.reparent(root, a), "cannot reparent the root");
        assert!(t.reparent(b, root), "valid reparent should succeed");
        assert_eq!(t.parent(b), root);
        check_invariants(&t);
    }

    #[test]
    fn hit_test_z_and_document_order() {
        let mut t = Tree::new();
        let root = t.insert_root();
        t.set_flags(root, NodeFlags::VISIBLE | NodeFlags::HIT_TESTABLE);
        t.set_bounds(root, rect(0.0, 0.0, 100.0, 100.0));
        // two overlapping children at the same point
        let a = t.insert_child(root);
        let b = t.insert_child(root);
        for n in [a, b] {
            t.set_flags(n, NodeFlags::VISIBLE | NodeFlags::HIT_TESTABLE);
            t.set_bounds(n, rect(10.0, 10.0, 30.0, 30.0));
        }
        let p = Point::new(20.0, 20.0);
        // equal z: later in document order (b) wins
        assert_eq!(t.hit_test(p), Some(b));
        // raise a's z above b: a wins despite earlier document order
        t.set_z(a, 5);
        assert_eq!(t.hit_test(p), Some(a));
        // a point outside the children falls through to the root
        assert_eq!(t.hit_test(Point::new(80.0, 80.0)), Some(root));
        check_invariants(&t);
    }

    #[test]
    fn hit_test_respects_clip_and_flags() {
        let mut t = Tree::new();
        let root = t.insert_root();
        t.set_flags(root, NodeFlags::VISIBLE | NodeFlags::HIT_TESTABLE);
        t.set_bounds(root, rect(0.0, 0.0, 100.0, 100.0));
        t.set_clip(root, Some(rect(0.0, 0.0, 50.0, 50.0)));
        let child = t.insert_child(root);
        t.set_flags(child, NodeFlags::VISIBLE | NodeFlags::HIT_TESTABLE);
        t.set_bounds(child, rect(40.0, 40.0, 40.0, 40.0)); // extends past clip
                                                           // inside clip and child
        assert_eq!(t.hit_test(Point::new(45.0, 45.0)), Some(child));
        // inside child but outside the root's clip -> nothing
        assert_eq!(t.hit_test(Point::new(70.0, 70.0)), None);
        // non-hit-testable node is skipped
        t.set_flags(child, NodeFlags::VISIBLE);
        assert_eq!(t.hit_test(Point::new(45.0, 45.0)), Some(root));
        check_invariants(&t);
    }

    // ----- property tests --------------------------------------------------

    #[derive(Debug, Clone)]
    enum Op {
        Insert(usize),
        Remove(usize),
        Reparent(usize, usize),
        SetZ(usize, u32),
        SetFlags(usize, u8),
        SetBounds(usize, u8, u8, u8, u8),
        SetClip(usize, Option<(u8, u8, u8, u8)>),
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            any::<usize>().prop_map(Op::Insert),
            any::<usize>().prop_map(Op::Remove),
            (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Op::Reparent(a, b)),
            (any::<usize>(), 0u32..8).prop_map(|(a, z)| Op::SetZ(a, z)),
            (any::<usize>(), any::<u8>()).prop_map(|(a, f)| Op::SetFlags(a, f)),
            (
                any::<usize>(),
                any::<u8>(),
                any::<u8>(),
                any::<u8>(),
                any::<u8>()
            )
                .prop_map(|(a, x, y, w, h)| Op::SetBounds(a, x, y, w, h)),
            (
                any::<usize>(),
                proptest::option::of((any::<u8>(), any::<u8>(), any::<u8>(), any::<u8>()))
            )
                .prop_map(|(a, c)| Op::SetClip(a, c)),
        ]
    }

    fn apply(t: &mut Tree, live: &mut Vec<NodeIndex>, op: &Op) {
        let pick = |live: &Vec<NodeIndex>, i: usize| -> Option<NodeIndex> {
            if live.is_empty() {
                None
            } else {
                Some(live[i % live.len()])
            }
        };
        match *op {
            Op::Insert(i) => {
                if t.root().is_none() {
                    live.push(t.insert_root());
                } else if let Some(p) = pick(live, i) {
                    live.push(t.insert_child(p));
                }
            }
            Op::Remove(i) => {
                if let Some(n) = pick(live, i) {
                    let mut doomed = Vec::new();
                    t.collect_subtree(n, &mut doomed);
                    t.remove(n);
                    live.retain(|x| !doomed.contains(x));
                }
            }
            Op::Reparent(i, j) => {
                if let (Some(n), Some(p)) = (pick(live, i), pick(live, j)) {
                    t.reparent(n, p);
                }
            }
            Op::SetZ(i, z) => {
                if let Some(n) = pick(live, i) {
                    t.set_z(n, z);
                }
            }
            Op::SetFlags(i, f) => {
                if let Some(n) = pick(live, i) {
                    t.set_flags(n, NodeFlags::from_bits_truncate(f as u32));
                }
            }
            Op::SetBounds(i, x, y, w, h) => {
                if let Some(n) = pick(live, i) {
                    t.set_bounds(n, rect(x as f64, y as f64, w as f64, h as f64));
                }
            }
            Op::SetClip(i, c) => {
                if let Some(n) = pick(live, i) {
                    t.set_clip(
                        n,
                        c.map(|(x, y, w, h)| rect(x as f64, y as f64, w as f64, h as f64)),
                    );
                }
            }
        }
    }

    proptest! {
        // 1024 cases each: random_edits applies ~tens of edits per case (>>10k
        // total, invariants checked after each), and hit_test_matches runs
        // against >1k distinct random scenes.
        #![proptest_config(ProptestConfig::with_cases(1024))]

        // Each case applies a batch of edits; across the case count this is well
        // over 10k random edits, with invariants checked after each.
        #[test]
        fn random_edits_preserve_invariants(ops in prop::collection::vec(op_strategy(), 0..80)) {
            let mut t = Tree::new();
            let mut live = Vec::new();
            for op in &ops {
                apply(&mut t, &mut live, op);
                check_invariants(&t);
            }
        }

        // Build a random scene, then assert the optimized hit-test agrees with
        // the naive oracle at a random point. Many cases => 1k+ random scenes.
        #[test]
        fn hit_test_matches_naive(
            ops in prop::collection::vec(op_strategy(), 0..40),
            px in 0u8..120, py in 0u8..120,
        ) {
            let mut t = Tree::new();
            let mut live = Vec::new();
            for op in &ops {
                apply(&mut t, &mut live, op);
            }
            let p = Point::new(px as f64, py as f64);
            prop_assert_eq!(t.hit_test(p), hit_test_naive(&t, p));
        }
    }
}
