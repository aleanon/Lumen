//! O2.1: a node faded to nothing stops reporting itself as healthy (W0111).
//!
//! `SemanticsNode` carries `bounds`, `ink`, `states`, `text_metrics` — and no
//! opacity or colour at all. So an `opacity: 0` button was invisible on screen,
//! correctly sized in the tree, hit-testable, labelled, and reported as fine by
//! every tool. Compare `visibility: hidden`, which the runtime handles
//! properly: the node loses its flags and leaves paint *and* semantics, so what
//! the agent sees matches what the user sees.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};

fn w0111(lss: &str) -> Vec<String> {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![
            widgets::button("Save", |_| {}).id("save"),
            widgets::text("caption").id("cap"),
        ])
        .id("group")
    })
    .run_headless(Size::new(300.0, 200.0));
    h.set_stylesheet(lss);
    h.pump();
    h.lint()
        .into_iter()
        .filter(|d| d.code == "W0111")
        .map(|d| d.message)
        .collect()
}

#[test]
fn a_fully_transparent_button_is_reported() {
    let found = w0111("#save { opacity: 0; }");
    assert_eq!(
        found.len(),
        1,
        "the invisible button is the finding: {found:?}"
    );
    assert!(
        found[0].contains("#save"),
        "it must name the node: {}",
        found[0]
    );
}

/// The case the whole "effective opacity" machinery exists for: the node's own
/// opacity is 1, and it is still completely invisible. No value anywhere in the
/// runtime held this before — paint composites nested `PushLayer`s and never
/// stores the product.
#[test]
fn a_child_of_a_transparent_group_is_reported_with_its_cause() {
    let found = w0111("#group { opacity: 0; }");
    assert!(
        found.iter().any(|m| m.contains("#save")),
        "an opaque child of a transparent group is invisible too: {found:?}"
    );
    let save = found.iter().find(|m| m.contains("#save")).unwrap();
    assert!(
        save.contains("enclosing group"),
        "the message must explain WHY, since the author's own opacity is 1: {save}"
    );
}

#[test]
fn an_opaque_ui_is_not_reported() {
    assert!(
        w0111("#save { opacity: 1; }").is_empty(),
        "a normal UI must stay silent"
    );
}

/// A faint-but-visible node is a design choice, not a defect. The floor is
/// "invisible", not "low".
#[test]
fn a_merely_faint_node_is_not_reported() {
    assert!(
        w0111("#save { opacity: 0.3; }").is_empty(),
        "0.3 is dim, not invisible"
    );
}

/// An overlay anchors to the window, not to its structural parent, so it must
/// NOT inherit a dimmed page's alpha — otherwise the one thing the user can
/// actually see would report itself as invisible.
#[test]
fn an_overlay_does_not_inherit_a_faded_ancestor() {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        let mut sheet = widgets::column(vec![widgets::button("Confirm", |_| {}).id("confirm")]);
        sheet.overlay = true;
        widgets::column(vec![widgets::text("page").id("page"), sheet]).id("group")
    })
    .run_headless(Size::new(300.0, 200.0));
    h.set_stylesheet("#group { opacity: 0; }");
    h.pump();
    let found: Vec<String> = h
        .lint()
        .into_iter()
        .filter(|d| d.code == "W0111")
        .map(|d| d.message)
        .collect();
    assert!(
        !found.iter().any(|m| m.contains("#confirm")),
        "the overlay paints above the faded page and is visible: {found:?}"
    );
}

/// The query side: `ui.getLayout` reports effective opacity, so an agent can
/// check a node it already suspects without waiting for the ambient audit.
#[test]
fn get_layout_reports_effective_opacity() {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![widgets::button("Save", |_| {}).id("save")]).id("group")
    })
    .run_headless(Size::new(300.0, 200.0));
    h.set_stylesheet("#group { opacity: 0.5; } #save { opacity: 0.5; }");
    h.pump();
    let o = h.node_opacity("#save").expect("resolves");
    assert!(
        (o - 0.25).abs() < 1e-3,
        "0.5 inside 0.5 composites to 0.25, not 0.5 -- the product is what the \
         user sees and is stored nowhere else: got {o}"
    );
}
