//! MOD2: the layout seam must be implementable by someone who is not Lumen.
//!
//! A trait the framework declares and only the framework implements is not an
//! extension point — it is an interface with one caller and one callee, which
//! proves nothing about substitutability. So this test implements a complete
//! alternative engine from outside the crate, using only public API.
//!
//! It is intentionally trivial (a fixed grid, no flex), because the property
//! under test is "can this be written at all", not "is it a good engine".

use kurbo::{Rect, Size};
use lumen_layout::{Dim, LayoutEngine, LayoutNode, LayoutStyle, LayoutTree};
use std::collections::HashMap;

/// A stack-of-rows engine: every node is 100x20, stacked vertically.
#[derive(Default)]
struct StackEngine {
    kids: Vec<Vec<usize>>,
    bounds: HashMap<usize, Rect>,
    touched: usize,
}

impl StackEngine {
    fn push(&mut self, children: &[LayoutNode]) -> LayoutNode {
        let idx = self.kids.len();
        self.kids
            .push(children.iter().map(|c| c.raw() as usize).collect());
        LayoutNode::from_raw(idx as u64)
    }
    fn place(&mut self, node: usize, x: f64, mut y: f64) -> f64 {
        let kids = self.kids[node].clone();
        let start = y;
        if kids.is_empty() {
            self.bounds
                .insert(node, Rect::new(x, y, x + 100.0, y + 20.0));
            return y + 20.0;
        }
        for k in kids {
            y = self.place(k, x, y);
        }
        self.bounds.insert(node, Rect::new(x, start, x + 100.0, y));
        y
    }
}

impl LayoutEngine for StackEngine {
    fn leaf(&mut self, _style: &LayoutStyle) -> LayoutNode {
        self.push(&[])
    }
    fn container(&mut self, _style: &LayoutStyle, children: &[LayoutNode]) -> LayoutNode {
        self.push(children)
    }
    fn set_style(&mut self, _node: LayoutNode, _style: &LayoutStyle) {}
    fn compute(&mut self, root: LayoutNode, _available: Size) {
        self.bounds.clear();
        self.place(root.raw() as usize, 0.0, 0.0);
        self.touched = self.bounds.len();
    }
    fn bounds(&self, node: LayoutNode) -> Rect {
        self.bounds
            .get(&(node.raw() as usize))
            .copied()
            .unwrap_or_default()
    }
    fn mirror_rtl(&mut self, _root: LayoutNode) {}
    fn touched(&self) -> usize {
        self.touched
    }
}

/// Drive any engine through the same script, so the seam is exercised
/// generically rather than against a concrete type.
fn stack_two_rows<E: LayoutEngine>(e: &mut E) -> (LayoutNode, LayoutNode, LayoutNode) {
    // Explicit sizes: the built-in engine solves real constraints, so a
    // default (unsized, empty) style correctly computes to zero. StackEngine
    // ignores style entirely, which is fine — the script has to be meaningful
    // for BOTH, or it tests only the one that happens to agree with it.
    let row = || LayoutStyle {
        width: Dim::px(100.0),
        height: Dim::px(20.0),
        ..LayoutStyle::default()
    };
    let a = e.leaf(&row());
    let b = e.leaf(&row());
    let root = e.container(
        &LayoutStyle {
            width: Dim::px(100.0),
            height: Dim::px(40.0),
            ..LayoutStyle::default()
        },
        &[a, b],
    );
    e.compute(root, Size::new(400.0, 400.0));
    (root, a, b)
}

#[test]
fn a_third_party_engine_satisfies_the_seam() {
    let mut e = StackEngine::default();
    let (root, a, b) = stack_two_rows(&mut e);

    assert_eq!(e.bounds(a).height(), 20.0);
    assert!(
        e.bounds(b).y0 >= e.bounds(a).y1,
        "the second row should sit below the first"
    );
    assert!(e.bounds(root).height() >= 40.0, "root spans its children");
    assert!(e.touched() > 0);
}

#[test]
fn the_builtin_engine_satisfies_the_same_seam() {
    // The same generic script against LayoutTree. If this compiled only for
    // the built-in, the trait would be describing taffy rather than layout.
    let mut t = LayoutTree::new();
    let (root, _a, _b) = stack_two_rows(&mut t);
    assert!(
        LayoutEngine::bounds(&t, root).width() > 0.0,
        "the built-in engine must produce real bounds through the trait"
    );
}
