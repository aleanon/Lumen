//! The piece the prototype was missing: a **heterogeneous child list**, lowered
//! into the retained tree with no `Element` anywhere.
//!
//! `Direct` originally read `fn lower(self, ..)`, which cannot be called
//! through `Box<dyn Direct>` — `dyn Direct` has no statically known size to
//! move out of (E0161). So a container could only hold children whose concrete
//! types it knew at compile time, and the prototype's own `begin_row` said as
//! much: *"the child widgets are known statically at each site"*. That is true
//! of a hand-written composite and false of every real view, all of which are
//! `column(vec![…])` over a mixed list.
//!
//! `self: Box<Self>` fixes it, and these tests hold the properties that make
//! the fix load-bearing rather than cosmetic.

use lumen_core::semantics::Role;
use lumen_widgets::direct::{node, Column, Node, StyleEnv, TreeSink, VisualState};
use lumen_widgets::{Button, Label, ProgressBar};

fn sink(src: &str) -> TreeSink {
    TreeSink::new().with_styles(
        StyleEnv::from_source(src).expect("parses"),
        VisualState::default(),
    )
}

/// The headline property: three different widget types in one `Vec`, written
/// straight into the tree. Under the old signature this did not compile.
#[test]
fn a_mixed_child_list_lowers_with_no_element() {
    let mut s = sink("");
    let kids: Vec<Node> = vec![
        node(Label::new("alpha")),
        node(Button::new("Go")),
        node(ProgressBar::new(0.25)),
        node(Label::new("omega")),
    ];
    let (root, _) = node(Column::new(kids).gap(4.0)).lower(&mut s, None);

    // Four children under one column, in declaration order.
    let mut kid = s.tree.first_child(root);
    let mut roles = Vec::new();
    while kid.is_some() {
        roles.push(s.meta.role(kid));
        kid = s.tree.next_sibling(kid);
    }
    assert_eq!(
        roles.len(),
        4,
        "every child in the vector reached the tree, got {roles:?}"
    );
    assert_eq!(roles[0], Role::Text, "a Label lowered as itself");
    assert_eq!(roles[1], Role::Button, "a Button lowered as itself");
}

/// Nesting: a column of columns. The container is itself a `Node`, so trees
/// compose to arbitrary depth without a uniform node type in between.
#[test]
fn containers_nest_as_ordinary_children() {
    let mut s = sink("");
    let inner: Vec<Node> = vec![node(Label::new("a")), node(Label::new("b"))];
    let outer: Vec<Node> = vec![
        node(Label::new("header")),
        node(Column::new(inner)),
        node(Button::new("footer")),
    ];
    let (root, _) = node(Column::new(outer)).lower(&mut s, None);

    let mid = s.tree.next_sibling(s.tree.first_child(root));
    assert_eq!(
        s.meta.role(mid),
        Role::Group,
        "the nested column is a group"
    );
    let mut n = 0;
    let mut c = s.tree.first_child(mid);
    while c.is_some() {
        n += 1;
        c = s.tree.next_sibling(c);
    }
    assert_eq!(n, 2, "the nested column kept its own two children");
}

/// The cascade must still see the container as an ancestor of its children —
/// children are lowered while the parent node is `Open`, which is exactly what
/// the typestate guards enforce. A descendant selector is the observable proof.
#[test]
fn a_descendant_selector_matches_through_a_dynamic_child_list() {
    let mut s = sink("group text { width: 320px; }");
    let kids: Vec<Node> = vec![node(Label::new("inside")), node(Label::new("also"))];
    let (root, _) = node(Column::new(kids)).lower(&mut s, None);

    let first = s.tree.first_child(root);
    assert_eq!(
        s.meta.layout_style(first).width,
        lumen_layout::Dim::px(320.0),
        "`group text` matched, so the column really was on the ancestor stack \
         while its children lowered"
    );
}
