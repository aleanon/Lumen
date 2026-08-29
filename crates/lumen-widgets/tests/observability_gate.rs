//! A11Y3 — turning `dev-observability` off must cost the *agent* payload and
//! nothing else.
//!
//! `ink`, `text_metrics`, `deps` and `type_name` are read by `lumen-agent` and
//! by the (equally gated) W0104 clipping audit, and by nothing else. Gating
//! them shrinks `SemanticsNode` from 432 to 320 bytes. The risk of a gate like
//! this is not that it fails to compile — it is that it quietly takes
//! something a *screen reader* needs with it, which no compile error would
//! catch and which would only show up on someone's machine.
//!
//! So these assertions run in **both** feature states, unchanged. That is the
//! same shape as `accessibility_gate.rs` (A11Y1), and for the same reason:
//! "turning it off is invisible to everything except the agent" is a claim
//! that has to be checked in the state where it could break.

use kurbo::Size;
use lumen_core::semantics::{Role, SemanticsNode};
use lumen_widgets::{widgets, App, BuildCx, Element, VirtualList};

fn find<'a>(n: &'a SemanticsNode, f: &dyn Fn(&SemanticsNode) -> bool) -> Option<&'a SemanticsNode> {
    if f(n) {
        return Some(n);
    }
    n.children.iter().find_map(|c| find(c, f))
}

/// Everything an assistive technology reads is still there.
#[test]
fn the_assistive_tech_contract_survives_the_gate() {
    let mut h = App::new(|_cx: &mut BuildCx| {
        widgets::column(vec![
            widgets::button("Save", |_| {}).id("save"),
            widgets::text("hello").id("greeting"),
        ])
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    let root = h.semantics_elided();

    let btn = find(&root, &|n| n.role == Role::Button).expect("role survives");
    assert_eq!(btn.label, "Save", "label survives");
    assert!(!btn.actions.is_empty(), "declared actions survive");
    assert!(btn.bounds.width() > 0.0, "bounds survive");
    assert_eq!(
        btn.id.as_ref().map(|i| i.as_str()),
        Some("save"),
        "ids survive — they are how anything addresses a node"
    );
    let txt = find(&root, &|n| n.role == Role::Text).expect("text node");
    assert_eq!(txt.label, "hello");
}

/// …including the virtualization contract, which is newer than the gate and
/// therefore the easiest thing to have swept up by it.
#[test]
fn the_virtualization_contract_survives_the_gate() {
    let mut h = App::new(|cx: &mut BuildCx| -> Element {
        VirtualList::new(cx, "vl", 100_000, 24.0, 300.0, |i| {
            widgets::text(format!("Row {i}"))
        })
        .into()
    })
    .run_headless(Size::new(400.0, 300.0));
    h.pump();
    let root = h.semantics_elided();
    let list = find(&root, &|n| n.role == Role::List).expect("list");
    assert_eq!(
        list.set_size,
        Some(100_000),
        "set_size is AT-facing, not agent-facing, and must survive"
    );
    fn any_pos(n: &SemanticsNode) -> bool {
        n.position_in_set.is_some() || n.children.iter().any(any_pos)
    }
    assert!(any_pos(&root), "position_in_set survives too");
}

/// Selector addressing — how `lumen-test` and the agent find nodes — is not
/// part of the payload and must not move.
#[test]
fn addressing_survives_the_gate() {
    let mut h = App::new(|_cx: &mut BuildCx| {
        widgets::column(vec![widgets::text("x").id("target").class("marked")])
    })
    .run_headless(Size::new(200.0, 100.0));
    h.pump();
    let root = h.semantics_elided();
    let n = find(&root, &|n| {
        n.id.as_ref().map(|i| i.as_str()) == Some("target")
    })
    .expect("addressable by id");
    assert!(
        n.classes.iter().any(|c| c == "marked"),
        "classes survive: they are addressing, not payload"
    );
}

/// The payload itself, asserted only where it is supposed to exist. With the
/// feature off these fields do not exist at all, so the test is compiled out
/// rather than inverted — an inverted assertion would silently pass if the
/// field were later restored but never populated.
#[cfg(feature = "dev-observability")]
#[test]
fn the_agent_payload_is_present_when_the_feature_is_on() {
    let mut h = App::new(|_cx: &mut BuildCx| widgets::text("hello").id("t"))
        .run_headless(Size::new(200.0, 100.0));
    h.pump();
    let root = h.semantics_elided();
    let n = find(&root, &|n| n.role == Role::Text).expect("text");
    assert!(
        n.ink.is_some(),
        "a painted text node records its glyph ink for the agent and W0104"
    );
    assert!(n.text_metrics.is_some(), "…and its text metrics");
    assert!(!n.type_name.is_empty(), "…and its widget type name");
}
