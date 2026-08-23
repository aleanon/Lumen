//! O0.1b: diagnostics carry a machine-readable node anchor.
//!
//! `Diagnostic` has always had a `node: Option<StableId>` field and a
//! `.with_node()` builder, and **nothing in the tree ever called it** — every
//! check embedded the offending node as free text inside `message`. Two
//! consequences: a consumer had to guess at a per-check formatting convention
//! to recover the node, and any dedup keyed on `(code, node)` collapsed every
//! finding of a code onto a single slot.
//!
//! `handle` exists separately from `node` because `node` is the *author's* id
//! and is absent on any unnamed node — including, by definition, every W0301.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};

fn lint_of(lss: &str) -> Vec<lumen_core::Diagnostic> {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![
            widgets::text("first overflowing label").id("a"),
            widgets::text("second overflowing label").id("b"),
        ])
        .id("root")
    })
    .run_headless(Size::new(400.0, 200.0));
    h.set_stylesheet(lss);
    h.pump();
    h.lint()
}

/// The regression the ambient audit depends on: two nodes with the SAME code
/// must be distinguishable. Before O0.1b both carried `node: None`, so a
/// `(code, node)` dedup key kept the first and silently dropped the second —
/// and "a list of cards that are all slightly off-screen" is the common case,
/// not the exotic one.
#[test]
fn two_findings_of_one_code_carry_distinct_anchors() {
    let findings = lint_of("#root { width: 400px; } #a { width: 600px; } #b { width: 600px; }");
    let overflow: Vec<_> = findings.iter().filter(|d| d.code == "W0103").collect();
    assert!(
        overflow.len() >= 2,
        "both children should overflow the 400px root: {findings:?}"
    );

    let handles: Vec<&str> = overflow
        .iter()
        .filter_map(|d| d.handle.as_deref())
        .collect();
    assert_eq!(
        handles.len(),
        overflow.len(),
        "every node-anchored finding carries a handle: {overflow:?}"
    );
    let unique: std::collections::HashSet<&&str> = handles.iter().collect();
    assert_eq!(
        unique.len(),
        handles.len(),
        "handles must distinguish the nodes, not collapse them: {handles:?}"
    );

    // The author's id rides along when there is one, since it is what the
    // author will recognise and what works as a selector.
    let ids: std::collections::HashSet<&str> = overflow
        .iter()
        .filter_map(|d| d.node.as_ref().map(|i| i.as_str()))
        .collect();
    assert!(
        ids.contains("a") && ids.contains("b"),
        "author ids attached where present: {overflow:?}"
    );
}

/// An unnamed focusable has no author id *by definition* — the missing name is
/// the finding. So this is the case that proves `handle` had to be a separate
/// field rather than a better-populated `node`.
#[test]
fn an_unnamed_focusable_is_still_addressable() {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        // No `.id()` and no label: nothing for `Diagnostic.node` to hold.
        let mut anonymous = widgets::button("", |_| {});
        anonymous.label = String::new();
        widgets::column(vec![anonymous]).id("root")
    })
    .run_headless(Size::new(400.0, 200.0));
    h.pump();
    let findings = h.lint();
    let d = findings
        .iter()
        .find(|d| d.code == "W0301")
        .unwrap_or_else(|| panic!("an unlabelled button must raise W0301: {findings:?}"));
    assert!(
        d.node.is_none(),
        "an unnamed focusable has no author id to attach: {d:?}"
    );
    assert!(
        d.handle.as_deref().is_some_and(|h| h.starts_with("nx-")),
        "...but must still be addressable by handle -- this is the case that \
         makes `handle` a separate field rather than a better-filled `node`: {d:?}"
    );
}

/// The rendered form must show the anchor too — a reader of the string should
/// not have to re-parse the message to tell two findings apart.
#[test]
fn display_shows_the_anchor() {
    let findings = lint_of("#root { width: 400px; } #a { width: 600px; }");
    let d = findings
        .iter()
        .find(|d| d.code == "W0103")
        .expect("an overflow finding");
    let shown = d.to_string();
    assert!(
        shown.contains("[#a]"),
        "the rendered diagnostic names its node: {shown}"
    );
}
