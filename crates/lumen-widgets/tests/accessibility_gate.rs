//! A11Y1: `accessibility` gates the OS **publisher**, never the semantics tree.
//!
//! `SemanticsNode` is Lumen's observability contract, not an accessibility
//! detail — the agent protocol (`ui.getTree`), `ui.lint`, `lumen-test`,
//! `audit.rs`, `wcag.rs` and the golden ladder all read it. Twelve modules
//! consume it; exactly one of them publishes to the OS. Gating the tree would
//! break every one of the other eleven, so the feature must not touch it.
//!
//! These tests compile in BOTH feature states and assert the same things, which
//! is the point: turning the feature off must be invisible to anything except
//! the OS bridge. A test that only ran in one state could not make that claim.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};

fn view(cx: &mut BuildCx) -> Element {
    let _ = cx;
    widgets::column(vec![
        widgets::text("hello").id("lbl"),
        widgets::button("Save", |_| {}).id("btn"),
    ])
}

/// The tree still exists, is populated, and carries roles and labels.
#[test]
fn semantics_survive_either_feature_state() {
    let mut h = App::new(view).run_headless(Size::new(300.0, 200.0));
    h.pump();
    let json = h.semantics_json().to_string();
    assert!(
        json.contains("hello"),
        "label missing from semantics: {json}"
    );
    assert!(json.contains("Save"), "button label missing: {json}");
    assert!(json.contains("button"), "role missing: {json}");
}

/// Selector lookup — what `lumen-test` and the agent's `input.click` are built
/// on — must keep working. This is the capability that would silently die if
/// the gate ever reached the tree instead of the publisher.
#[test]
fn selector_lookup_survives_either_feature_state() {
    let mut h = App::new(view).run_headless(Size::new(300.0, 200.0));
    h.pump();
    assert!(
        h.node_bounds_by_id("btn").is_some(),
        "selector lookup broke — the agent protocol and every headless test \
         depend on this"
    );
}

/// The elided tree (what the OS publisher would consume) is still buildable
/// with the feature off. Only *publishing* is gated, so the projection that
/// feeds it must remain available — `lumen-shell-android` and the audit paths
/// use it too.
#[test]
fn the_elided_projection_is_not_gated() {
    let mut h = App::new(view).run_headless(Size::new(300.0, 200.0));
    h.pump();
    let elided = h.semantics_elided();
    assert!(
        !elided.children.is_empty(),
        "the elided semantics projection came back empty"
    );
}

/// The AccessKit mapping is the part that IS gated, so it may only be nameable
/// when the feature is on. If this compiled with the feature off, the gate
/// would not be removing the dependency at all.
#[cfg(feature = "accessibility")]
#[test]
fn the_accesskit_bridge_exists_only_when_enabled() {
    let mut h = App::new(view).run_headless(Size::new(300.0, 200.0));
    h.pump();
    let update = lumen_widgets::a11y::build_tree(&h.semantics_elided());
    assert!(
        !update.nodes.is_empty(),
        "the AccessKit tree builder produced no nodes"
    );
}
