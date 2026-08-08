//! T4.3 acceptance: the AccessKit tree built from Lumen's semantics matches the
//! expected roles/labels/states (an AccessKit-tree diff), and the role map is
//! complete (enforced by the exhaustive match in `a11y::role_to_accesskit`).

use accesskit::{Role as AkRole, Toggled};
use kurbo::Size;
use lumen_core::semantics::Role;
use lumen_widgets::a11y::{build_tree, role_to_accesskit};
use lumen_widgets::{widgets, App, BuildCx, Element, Headless};

fn run(build: impl Fn(&mut BuildCx) -> Element + 'static) -> Headless {
    App::new(build).run_headless(Size::new(300.0, 240.0))
}

#[test]
fn role_map_covers_representative_roles() {
    assert_eq!(role_to_accesskit(Role::Button), AkRole::Button);
    assert_eq!(role_to_accesskit(Role::Checkbox), AkRole::CheckBox);
    assert_eq!(role_to_accesskit(Role::TextInput), AkRole::TextInput);
    assert_eq!(role_to_accesskit(Role::Table), AkRole::Table);
    assert_eq!(role_to_accesskit(Role::TreeItem), AkRole::TreeItem);
    assert_eq!(role_to_accesskit(Role::Generic), AkRole::GenericContainer);
}

#[test]
fn accesskit_tree_matches_semantics() {
    let mut h = run(|cx| {
        widgets::column(vec![
            widgets::button("Save", |_| {}).id("save"),
            widgets::switch(cx, "wifi", "Wi-Fi").id("wifi"),
        ])
    });
    // Toggle the switch on so it carries a checked state.
    h.pump();

    let doc = h.semantics_doc();
    let elided = doc.root.elided();
    let update = build_tree(&elided);

    // Same node count as the semantics tree.
    fn count(n: &lumen_core::semantics::SemanticsNode) -> usize {
        1 + n.children.iter().map(count).sum::<usize>()
    }
    assert_eq!(update.nodes.len(), count(&elided));

    // There is a Button labelled "Save".
    let button = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == AkRole::Button)
        .expect("button node");
    assert_eq!(button.1.label(), Some("Save"));

    // The switch maps to a Switch with a toggled state.
    let sw = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == AkRole::Switch)
        .expect("switch node");
    assert!(matches!(
        sw.1.toggled(),
        Some(Toggled::True | Toggled::False)
    ));

    // The tree update has a root and a valid focus target.
    assert!(update.tree.is_some());
    assert!(update.nodes.iter().any(|(id, _)| *id == update.focus));
}

#[test]
fn tree_widget_exposes_expanded_state() {
    use lumen_widgets::widgets::TreeRow;
    let rows = [
        TreeRow {
            id: "a",
            label: "A",
            depth: 0,
            has_children: true,
        },
        TreeRow {
            id: "b",
            label: "B",
            depth: 1,
            has_children: false,
        },
    ];
    let mut h = run(move |cx| widgets::tree(cx, "t", &rows));
    h.pump();
    let elided = h.semantics_doc().root.elided();
    let update = build_tree(&elided);

    let item = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == AkRole::TreeItem)
        .expect("tree item");
    // Collapsed by default → expanded == Some(false).
    assert_eq!(item.1.is_expanded(), Some(false));
}

/// P.4 acceptance: the adapter tree is a faithful projection of
/// `semantics_json` — every node walked in parallel matches on mapped role,
/// label, value, and (new) window-space bounds, and child order is preserved.
#[test]
fn adapter_tree_equals_semantics_tree_node_for_node() {
    use std::collections::HashMap;

    let mut h = run(|cx| {
        widgets::column(vec![
            widgets::text("Profile").id("title"),
            widgets::switch(cx, "wifi", "Wi-Fi").id("wifi"),
            widgets::button("Save", |_| {}).id("save"),
            widgets::text_field_basic(cx, "name", "Ada").id("name"),
        ])
    });
    h.pump();

    let elided = h.semantics_doc().root.elided();
    let update = build_tree(&elided);
    let by_id: HashMap<u64, &accesskit::Node> =
        update.nodes.iter().map(|(id, n)| (id.0, n)).collect();
    assert_eq!(by_id.len(), update.nodes.len(), "no duplicate node ids");

    // Walk both trees in parallel from their roots. Adapter ids are salted by
    // structural path (`(salt << 32) | node`) and deliberately do NOT equal the
    // semantics node index, so the trees are matched by *position* — which is
    // what "faithful projection" means — rather than by reconstructing the id
    // derivation.
    fn walk(
        n: &lumen_core::semantics::SemanticsNode,
        ak: &accesskit::Node,
        by_id: &HashMap<u64, &accesskit::Node>,
    ) {
        assert_eq!(
            ak.role(),
            role_to_accesskit(n.role),
            "role of node-{}",
            n.node
        );
        if !n.label.is_empty() {
            assert_eq!(
                ak.label(),
                Some(n.label.as_str()),
                "label of node-{}",
                n.node
            );
        }
        if let Some(v) = &n.value {
            assert_eq!(ak.value(), Some(v.as_str()), "value of node-{}", n.node);
        }
        let b = ak.bounds().expect("bounds published");
        assert_eq!(
            (b.x0, b.y0, b.x1, b.y1),
            (n.bounds.x0, n.bounds.y0, n.bounds.x1, n.bounds.y1),
            "bounds of node-{}",
            n.node
        );
        // Child count and order.
        let kids = ak.children();
        assert_eq!(
            kids.len(),
            n.children.len(),
            "child count of node-{}",
            n.node
        );
        for (c, ak_child_id) in n.children.iter().zip(kids) {
            let ak_child = by_id
                .get(&ak_child_id.0)
                .unwrap_or_else(|| panic!("child of node-{} missing from adapter tree", n.node));
            walk(c, ak_child, by_id);
        }
    }

    let root_id = update.tree.as_ref().expect("tree published").root;
    let ak_root = by_id.get(&root_id.0).expect("root present in adapter tree");
    walk(&elided, ak_root, &by_id);
}
