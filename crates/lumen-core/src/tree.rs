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
#![allow(dead_code)]

use crate::identity::NodeIndex;
use bitflags::bitflags;
use kurbo::{Affine, Point, Rect};

bitflags! {
    /// Per-node state bits stored in the SoA `flags` array (02 §5).
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub(crate) struct NodeFlags: u32 {
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
pub(crate) struct Tree {
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

    // intrusive tree links (02 §5)
    parent: Vec<NodeIndex>,
    first_child: Vec<NodeIndex>,
    next_sibling: Vec<NodeIndex>,

    root: NodeIndex,
}

impl Tree {
    /// An empty tree with no root.
    pub(crate) fn new() -> Tree {
        Tree {
            generation: Vec::new(),
            alive: Vec::new(),
            free: Vec::new(),
            live_count: 0,
            bounds: Vec::new(),
            transform: Vec::new(),
            opacity: Vec::new(),
            clip: Vec::new(),
            flags: Vec::new(),
            z: Vec::new(),
            parent: Vec::new(),
            first_child: Vec::new(),
            next_sibling: Vec::new(),
            root: NodeIndex::NONE,
        }
    }

    /// The root node, or [`NodeIndex::NONE`] if the tree is empty.
    pub(crate) fn root(&self) -> NodeIndex {
        self.root
    }

    /// Number of live nodes.
    pub(crate) fn len(&self) -> usize {
        self.live_count
    }

    /// Whether `n` refers to a currently-live node (generation must match).
    pub(crate) fn is_alive(&self, n: NodeIndex) -> bool {
        let i = n.index() as usize;
        n.is_some() && i < self.alive.len() && self.alive[i] && self.generation[i] == n.generation()
    }

    // --- structure mutation -------------------------------------------------

    /// Allocate the root. Panics if a root already exists.
    pub(crate) fn insert_root(&mut self) -> NodeIndex {
        assert!(self.root.is_none(), "tree already has a root");
        let n = self.alloc(NodeIndex::NONE);
        self.root = n;
        n
    }

    /// Allocate a node and append it as the last child of `parent`.
    /// `parent` must be live.
    pub(crate) fn insert_child(&mut self, parent: NodeIndex) -> NodeIndex {
        debug_assert!(self.is_alive(parent), "insert_child: dead parent");
        let n = self.alloc(parent);
        self.link_last_child(parent, n);
        n
    }

    /// Move `node` to become the last child of `new_parent`. Returns `false`
    /// (and does nothing) if the move is invalid — `node` is the root, either
    /// index is dead, or `new_parent` lies within `node`'s own subtree (which
    /// would create a cycle).
    pub(crate) fn reparent(&mut self, node: NodeIndex, new_parent: NodeIndex) -> bool {
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

    /// Remove `node` and its entire subtree, recycling all their slots.
    /// Removing the root empties the tree.
    pub(crate) fn remove(&mut self, node: NodeIndex) {
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
    /// semantics and `ui.getLayout` (02 §5).
    pub(crate) fn bounds(&self, n: NodeIndex) -> Rect {
        self.bounds[n.index() as usize]
    }
    pub(crate) fn set_bounds(&mut self, n: NodeIndex, r: Rect) {
        self.bounds[n.index() as usize] = r;
    }
    pub(crate) fn z(&self, n: NodeIndex) -> u32 {
        self.z[n.index() as usize]
    }
    pub(crate) fn set_z(&mut self, n: NodeIndex, z: u32) {
        self.z[n.index() as usize] = z;
    }
    pub(crate) fn flags(&self, n: NodeIndex) -> NodeFlags {
        self.flags[n.index() as usize]
    }
    pub(crate) fn set_flags(&mut self, n: NodeIndex, f: NodeFlags) {
        self.flags[n.index() as usize] = f;
    }
    pub(crate) fn set_clip(&mut self, n: NodeIndex, c: Option<Rect>) {
        self.clip[n.index() as usize] = c;
    }
    pub(crate) fn set_opacity(&mut self, n: NodeIndex, o: f32) {
        self.opacity[n.index() as usize] = o;
    }
    pub(crate) fn set_transform(&mut self, n: NodeIndex, t: Affine) {
        self.transform[n.index() as usize] = t;
    }

    // --- link accessors -----------------------------------------------------

    pub(crate) fn parent(&self, n: NodeIndex) -> NodeIndex {
        self.parent[n.index() as usize]
    }
    pub(crate) fn first_child(&self, n: NodeIndex) -> NodeIndex {
        self.first_child[n.index() as usize]
    }
    pub(crate) fn next_sibling(&self, n: NodeIndex) -> NodeIndex {
        self.next_sibling[n.index() as usize]
    }

    // --- iteration ----------------------------------------------------------

    /// Live nodes in document order (depth-first preorder from the root).
    pub(crate) fn document_order(&self) -> Vec<NodeIndex> {
        let mut out = Vec::with_capacity(self.live_count);
        if self.root.is_some() {
            self.visit_preorder(self.root, &mut out);
        }
        out
    }

    /// Live nodes in paint (z) order: document order stably sorted by ascending
    /// `z`. Lower `z` paints first; equal `z` keeps document order.
    pub(crate) fn z_order(&self) -> Vec<NodeIndex> {
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
    pub(crate) fn hit_test(&self, p: Point) -> Option<NodeIndex> {
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
            self.parent[iu] = parent;
            self.first_child[iu] = NodeIndex::NONE;
            self.next_sibling[iu] = NodeIndex::NONE;
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
            self.parent.push(parent);
            self.first_child.push(NodeIndex::NONE);
            self.next_sibling.push(NodeIndex::NONE);
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
        self.parent[i] = NodeIndex::NONE;
        self.free.push(n.index());
        self.live_count -= 1;
    }

    /// Append `child` to `parent`'s sibling list. `child` must be detached.
    fn link_last_child(&mut self, parent: NodeIndex, child: NodeIndex) {
        let pi = parent.index() as usize;
        self.parent[child.index() as usize] = parent;
        self.next_sibling[child.index() as usize] = NodeIndex::NONE;
        let head = self.first_child[pi];
        if head.is_none() {
            self.first_child[pi] = child;
            return;
        }
        let mut cur = head;
        loop {
            let next = self.next_sibling[cur.index() as usize];
            if next.is_none() {
                self.next_sibling[cur.index() as usize] = child;
                return;
            }
            cur = next;
        }
    }

    /// Detach `node` from its parent's child list (node itself stays alive).
    fn unlink(&mut self, node: NodeIndex) {
        let parent = self.parent[node.index() as usize];
        if parent.is_none() {
            return;
        }
        let pi = parent.index() as usize;
        let head = self.first_child[pi];
        if head == node {
            self.first_child[pi] = self.next_sibling[node.index() as usize];
        } else {
            let mut cur = head;
            while cur.is_some() {
                let next = self.next_sibling[cur.index() as usize];
                if next == node {
                    self.next_sibling[cur.index() as usize] =
                        self.next_sibling[node.index() as usize];
                    break;
                }
                cur = next;
            }
        }
        self.next_sibling[node.index() as usize] = NodeIndex::NONE;
        self.parent[node.index() as usize] = NodeIndex::NONE;
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
            while c.is_some() {
                assert_eq!(t.parent[c.index() as usize], n, "child's parent != n");
                c = t.next_sibling[c.index() as usize];
            }
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
