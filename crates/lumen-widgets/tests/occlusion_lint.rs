//! O2.3: a control buried under something opaque says so (W0113).
//!
//! Occlusion was checked in exactly one place before this: `ui.explain` with
//! `kind: "click"`, considering only nodes marked `overlay`, and only the single
//! centre point a synthesized click uses. An ordinary sibling raised by
//! `z-index`, or a panel that grew over its neighbour, was reported by nothing
//! — and `ui.explain` only answers about a node you already suspect.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};

fn w0113(lss: &str) -> Vec<String> {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::stack(vec![
            widgets::button("Save", |_| {}).id("save"),
            widgets::column(Vec::new()).id("cover"),
        ])
        .id("root")
    })
    .run_headless(Size::new(400.0, 300.0));
    h.set_stylesheet(lss);
    h.pump();
    h.lint()
        .into_iter()
        .filter(|d| d.code == "W0113")
        .map(|d| d.message)
        .collect()
}

#[test]
fn a_button_under_an_opaque_panel_is_reported() {
    let found = w0113(
        "#save { width: 120px; height: 40px; } \
         #cover { width: 400px; height: 300px; background: #ffffff; }",
    );
    assert_eq!(
        found.len(),
        1,
        "the buried button is the finding: {found:?}"
    );
    assert!(found[0].contains("#save"), "names the node: {}", found[0]);
    assert!(found[0].contains("#cover"), "names the cover: {}", found[0]);
}

/// A translucent scrim dims a control; it does not hide it. Reporting that
/// would fire on every modal backdrop in existence.
#[test]
fn a_translucent_cover_is_not_occlusion() {
    let found = w0113(
        "#save { width: 120px; height: 40px; } \
         #cover { width: 400px; height: 300px; background: #ffffff80; }",
    );
    assert!(found.is_empty(), "a scrim is not a defect: {found:?}");
}

/// Partial overlap is routine layout. Only near-total coverage means the
/// control cannot be reached.
#[test]
fn partial_overlap_is_not_reported() {
    let found = w0113(
        "#save { width: 400px; height: 300px; } \
         #cover { width: 40px; height: 20px; background: #ffffff; }",
    );
    assert!(found.is_empty(), "a small overlap is normal: {found:?}");
}

#[test]
fn a_normal_layout_is_not_reported() {
    let found = w0113("#save { width: 120px; height: 40px; }");
    assert!(found.is_empty(), "nothing covers anything: {found:?}");
}

/// A node's own ancestor drawing a background is its BACKDROP, not something
/// covering it — otherwise every button on a card would be reported.
#[test]
fn an_ancestors_background_is_not_occlusion() {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![widgets::button("Save", |_| {}).id("save")]).id("card")
    })
    .run_headless(Size::new(400.0, 300.0));
    h.set_stylesheet("#card { width: 400px; height: 300px; background: #ffffff; }");
    h.pump();
    let found: Vec<String> = h
        .lint()
        .into_iter()
        .filter(|d| d.code == "W0113")
        .map(|d| d.message)
        .collect();
    assert!(
        found.is_empty(),
        "a card is the button's backdrop, not its cover: {found:?}"
    );
}
