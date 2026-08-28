//! A widget that writes itself into the real engine, with no `Element` subtree.
//!
//! Everything before this ran against the prototype's own tree. These lower
//! through `lumen-app`'s `Sink`, into the same arena, layout tree and meta
//! table a frame is painted from — so the thing being tested is the engine's
//! lowering, not a model of it.
//!
//! The three properties that matter:
//!
//! * a `Direct` widget produces real, addressable, laid-out nodes;
//! * its children are **statements** — emitted while the parent is open,
//!   never collected into a `Vec` and never boxed;
//! * it composes inside an ordinary `Element` tree, which is what lets widgets
//!   convert one at a time instead of all at once.

use kurbo::Size;
use lumen_app::{Direct, NodeWriter};
use lumen_core::NodeIndex;
use lumen_layout::LayoutNode;
use lumen_widgets::{widgets, App, Element};

/// A list that emits `n` labelled rows. It holds a count, not children — there
/// is no `Vec<Element>` and no `Vec<Box<dyn ..>>` anywhere in it.
struct Rows {
    n: usize,
    tag: &'static str,
}

impl Direct for Rows {
    fn lower_owned(
        self,
        w: &mut dyn NodeWriter,
        parent: Option<NodeIndex>,
        in_overlay: bool,
    ) -> (NodeIndex, LayoutNode) {
        let Rows { n, tag } = self;
        let mut own: Element = widgets::column(Vec::new()).id(tag);
        own.children.clear();
        w.write_with(own, parent, in_overlay, &mut |w, node, overlay| {
            // Children as statements. Each row exists only for the instant it
            // takes to write it; nothing holds a tree.
            (0..n)
                .map(|i| {
                    let row: Element = widgets::text(format!("row {i}")).id(format!("{tag}{i}"));
                    w.write_leaf(row, Some(node), overlay).1
                })
                .collect()
        })
    }
}

#[test]
fn a_direct_widget_produces_real_addressable_nodes() {
    let mut h =
        App::new(|_cx| widgets::column(vec![Element::default().direct(Rows { n: 5, tag: "r" })]))
            .run_headless(Size::new(300.0, 400.0));
    h.pump();

    assert!(
        h.node_bounds_by_id("r").is_some(),
        "the Direct container is in the tree"
    );
    for i in 0..5 {
        let b = h
            .node_bounds_by_id(&format!("r{i}"))
            .unwrap_or_else(|| panic!("row {i} is missing"));
        assert!(b.width() > 0.0 && b.height() > 0.0, "row {i} was laid out");
    }
    h.assert_view_coherent();
}

/// Rows must stack vertically — i.e. the children really were written *under*
/// the Direct container, not hoisted somewhere else in the tree.
#[test]
fn direct_children_are_parented_and_laid_out_in_order() {
    let mut h =
        App::new(|_cx| widgets::column(vec![Element::default().direct(Rows { n: 3, tag: "q" })]))
            .run_headless(Size::new(300.0, 400.0));
    h.pump();

    let y = |i: usize| h.node_bounds_by_id(&format!("q{i}")).unwrap().y0;
    assert!(
        y(0) < y(1) && y(1) < y(2),
        "rows stack in declaration order"
    );
    h.assert_view_coherent();
}

/// The boundary: a `Direct` widget sits inside an ordinary `Element` tree,
/// beside ordinary widgets, and both lower into the same frame. This is the
/// property that makes the migration incremental — a widget can convert
/// without its parent converting first.
#[test]
fn a_direct_widget_composes_beside_element_widgets() {
    let mut h = App::new(|_cx| {
        widgets::column(vec![
            widgets::text("header").id("h"),
            Element::default().direct(Rows { n: 2, tag: "z" }),
            widgets::text("footer").id("f"),
        ])
    })
    .run_headless(Size::new(300.0, 400.0));
    h.pump();

    let head = h.node_bounds_by_id("h").expect("header");
    let z0 = h.node_bounds_by_id("z0").expect("direct row");
    let foot = h.node_bounds_by_id("f").expect("footer");
    assert!(
        head.y0 < z0.y0 && z0.y0 < foot.y0,
        "the Direct subtree is laid out in its declared position between the \
         two Element widgets: {head:?} {z0:?} {foot:?}"
    );
    h.assert_view_coherent();
}
