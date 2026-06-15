//! T0.8 acceptance: schema validation + a selector test table (≥30 cases).

use super::*;
use crate::identity::StableId;

fn node(n: u32, role: Role, label: &str) -> SemanticsNode {
    let mut s = SemanticsNode::new(n, role);
    s.label = label.to_string();
    s.type_name = format!("{role:?}");
    s
}
fn with_id(mut s: SemanticsNode, id: &str) -> SemanticsNode {
    s.id = Some(StableId::from(id));
    s
}
fn with_class(mut s: SemanticsNode, class: &str) -> SemanticsNode {
    s.classes.push(class.to_string());
    s
}

/// A representative UI:
/// window
///   ├ group (elided, pure layout)
///   │   ├ button#save .primary "Save"
///   │   ├ button#cancel "Cancel"
///   │   └ button "Continue"
///   ├ list
///   │   ├ list_item "invoice 001"
///   │   ├ list_item "receipt 002"
///   │   └ list_item "invoice 003"
///   └ dialog .modal
///       └ group .footer
///           ├ button "OK"
///           └ button "Apply"
fn fixture() -> SemanticsNode {
    let mut win = node(0, Role::Window, "");

    let mut row = node(1, Role::Group, "");
    row.elide = true; // pure layout, no contribution
    row.children = vec![
        with_class(with_id(node(2, Role::Button, "Save"), "save"), "primary"),
        with_id(node(3, Role::Button, "Cancel"), "cancel"),
        node(4, Role::Button, "Continue"),
    ];

    let mut list = node(5, Role::List, "");
    list.children = vec![
        node(6, Role::ListItem, "invoice 001"),
        node(7, Role::ListItem, "receipt 002"),
        node(8, Role::ListItem, "invoice 003"),
    ];

    let mut dialog = with_class(node(9, Role::Dialog, ""), "modal");
    let mut footer = with_class(node(10, Role::Group, ""), "footer");
    footer.children = vec![
        node(11, Role::Button, "OK"),
        node(12, Role::Button, "Apply"),
    ];
    dialog.children = vec![footer];

    win.children = vec![row, list, dialog];
    win
}

fn doc() -> SemanticsDoc {
    SemanticsDoc {
        window: WindowInfo {
            width: 800.0,
            height: 600.0,
            scale: 2.0,
            focused: Some(2),
        },
        root: fixture(),
    }
}

#[test]
fn elision_splices_pure_layout_children() {
    let raw = fixture();
    assert_eq!(
        raw.children.iter().map(|c| c.node).collect::<Vec<_>>(),
        vec![1, 5, 9]
    );
    let elided = raw.elided();
    // group(1) elided -> its buttons splice into window
    assert_eq!(
        elided.children.iter().map(|c| c.node).collect::<Vec<_>>(),
        vec![2, 3, 4, 5, 9]
    );
}

#[test]
fn document_validates_against_schema() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../schema/semantics-1.json")).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();

    for raw in [false, true] {
        let instance = doc().to_json(raw);
        if !validator.is_valid(&instance) {
            let errs: Vec<String> = validator
                .iter_errors(&instance)
                .map(|e| e.to_string())
                .collect();
            panic!("schema validation failed (raw={raw}): {errs:?}");
        }
    }
}

fn sel(selector: &str) -> Vec<u32> {
    let elided = fixture().elided();
    select(&elided, selector).unwrap_or_else(|e| panic!("parse failed for {selector:?}: {e}"))
}

#[test]
fn selector_table() {
    // id / class
    assert_eq!(sel("#save"), vec![2]);
    assert_eq!(sel("#cancel"), vec![3]);
    assert_eq!(sel(".primary"), vec![2]);
    assert_eq!(sel(".modal"), vec![9]);
    assert_eq!(sel("#nonexistent"), Vec::<u32>::new());

    // role
    assert_eq!(sel("button"), vec![2, 3, 4, 11, 12]);
    assert_eq!(sel("list"), vec![5]);
    assert_eq!(sel("list_item"), vec![6, 7, 8]);
    assert_eq!(sel("dialog"), vec![9]);
    assert_eq!(sel("group"), vec![10]); // group(1) was elided
    assert_eq!(sel(":checked"), Vec::<u32>::new());

    // compound
    assert_eq!(sel("button.primary"), vec![2]);
    assert_eq!(sel("button#cancel"), vec![3]);

    // :text and :text-contains
    assert_eq!(sel("button:text(\"Continue\")"), vec![4]);
    assert_eq!(sel("button:text(\"Save\")"), vec![2]);
    assert_eq!(sel(":text(\"Cancel\")"), vec![3]);
    assert_eq!(sel(":text-contains(\"inv\")"), vec![6, 8]);
    assert_eq!(sel("button:text-contains(\"a\")"), vec![2, 3, 12]);

    // combinators
    assert_eq!(sel("list > list_item"), vec![6, 7, 8]);
    assert_eq!(sel("window > button"), vec![2, 3, 4]);
    assert_eq!(sel("dialog button"), vec![11, 12]);
    assert_eq!(sel("dialog .footer > button"), vec![11, 12]);

    // :nth (applied last)
    assert_eq!(sel("button:nth(1)"), vec![2]);
    assert_eq!(sel("button:nth(5)"), vec![12]);
    assert_eq!(sel("list_item:nth(2)"), vec![7]);
    assert_eq!(sel("dialog .footer > button:nth(2)"), vec![12]);
    assert_eq!(sel("button:nth(99)"), Vec::<u32>::new());

    // :has (the inner selector must match a *descendant*)
    assert_eq!(sel("list:has(:text-contains(\"invoice\"))"), vec![5]);
    assert_eq!(sel("list:has(list_item)"), vec![5]);
    assert_eq!(sel("dialog:has(button:text(\"OK\"))"), vec![9]);
    // a list_item is a leaf here, so it has no matching descendant
    assert_eq!(
        sel("list_item:has(:text-contains(\"invoice\"))"),
        Vec::<u32>::new()
    );

    // wildcard
    assert_eq!(sel("*").len(), 12);
}

#[test]
fn resolve_one_single_match() {
    let elided = fixture().elided();
    assert_eq!(resolve_one(&elided, "#save"), Ok(2));
    assert_eq!(resolve_one(&elided, "button:text(\"Save\")"), Ok(2));
    assert_eq!(resolve_one(&elided, ".primary"), Ok(2));
}

#[test]
fn resolve_one_ambiguous_lists_candidates() {
    let elided = fixture().elided();
    match resolve_one(&elided, "button") {
        Err(ResolveError::Ambiguous { candidates }) => {
            assert_eq!(candidates, vec![2, 3, 4, 11, 12]);
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
}

#[test]
fn resolve_one_not_found_suggests_nearest() {
    let elided = fixture().elided();
    match resolve_one(&elided, "button#missing") {
        Err(ResolveError::NotFound { nearest }) => {
            // nearest-miss ignores the failing #missing structural constraint
            assert!(nearest.is_empty() || nearest.iter().all(|n| [2, 3, 4, 11, 12].contains(n)));
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn parse_errors() {
    let elided = fixture().elided();
    assert!(select(&elided, "").is_err());
    assert!(matches!(
        resolve_one(&elided, ""),
        Err(ResolveError::Parse(_))
    ));
}
