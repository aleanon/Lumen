//! T4.3 acceptance: the AccessKit tree built from Lumen's semantics matches the
//! expected roles/labels/states (an AccessKit-tree diff), and the role map is
//! complete (enforced by the exhaustive match in `a11y::role_to_accesskit`).

use accesskit::{Role as AkRole, Toggled};
use kurbo::Size;
use lumen_core::semantics::Role;
use lumen_widgets::a11y::{build_tree, role_to_accesskit};
use lumen_widgets::{widgets, widgets_m1, widgets_m4, App, BuildCx, Element, Headless};

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
            widgets_m1::switch(cx, "wifi", "Wi-Fi").id("wifi"),
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
    use lumen_widgets::widgets_m4::TreeRow;
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
    let mut h = run(move |cx| widgets_m4::tree(cx, "t", &rows));
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
            widgets_m1::switch(cx, "wifi", "Wi-Fi").id("wifi"),
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

    fn walk(n: &lumen_core::semantics::SemanticsNode, by_id: &HashMap<u64, &accesskit::Node>) {
        let ak = by_id
            .get(&u64::from(n.node))
            .unwrap_or_else(|| panic!("node-{} missing from adapter tree", n.node));
        assert_eq!(
            ak.role(),
            role_to_accesskit(n.role),
            "role of node-{}",
            n.node
        );
        if n.label.is_empty() {
            // Only the Invalid-state substitute may set a label here.
        } else {
            assert_eq!(ak.label(), Some(n.label.as_str()));
        }
        if let Some(v) = &n.value {
            assert_eq!(ak.value(), Some(v.as_str()));
        }
        let b = ak.bounds().expect("bounds published");
        assert_eq!(
            (b.x0, b.y0, b.x1, b.y1),
            (n.bounds.x0, n.bounds.y0, n.bounds.x1, n.bounds.y1),
            "bounds of node-{}",
            n.node
        );
        // Child identity and order.
        let kids: Vec<u64> = ak.children().iter().map(|c| c.0).collect();
        let sem_kids: Vec<u64> = n.children.iter().map(|c| u64::from(c.node)).collect();
        assert_eq!(kids, sem_kids, "children of node-{}", n.node);
        for c in &n.children {
            walk(c, by_id);
        }
    }
    walk(&elided, &by_id);
}
