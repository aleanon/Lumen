//! O2.4: a blank window says so (W0114).
//!
//! The most common early-development outcome, and the one every per-node lint
//! misses *by design*: each individual zero-area node is defensible — `W0105`
//! fires only on interactive ones, because a decorative spacer with no size is
//! not a defect — so a screen where everything collapsed passes every per-node
//! check while showing the user nothing at all. The semantic tree is fully
//! populated, so `ui.getTree` looks entirely healthy.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};

fn w0114(lss: &str) -> Vec<String> {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![
            widgets::text("title").id("t"),
            widgets::text("body").id("b"),
            widgets::text("footer").id("f"),
        ])
        .id("root")
    })
    .run_headless(Size::new(400.0, 300.0));
    h.set_stylesheet(lss);
    h.pump();
    h.lint()
        .into_iter()
        .filter(|d| d.code == "W0114")
        .map(|d| d.message)
        .collect()
}

#[test]
fn a_collapsed_layout_is_reported_as_a_blank_frame() {
    let found = w0114("#root, #t, #b, #f { width: 0px; height: 0px; }");
    assert_eq!(found.len(), 1, "one whole-frame finding: {found:?}");
    assert!(
        found[0].contains("blank"),
        "it must say the screen is empty, in those terms: {}",
        found[0]
    );
}

#[test]
fn a_normal_frame_is_not_reported() {
    assert!(
        w0114("#root { width: 400px; }").is_empty(),
        "a frame with content must stay silent"
    );
}

/// Exactly one finding, not one per collapsed node — the point is that the
/// *frame* is empty, and N identical complaints would bury it.
#[test]
fn it_reports_the_frame_not_each_node() {
    let found = w0114("#root, #t, #b, #f { width: 0px; height: 0px; }");
    assert_eq!(found.len(), 1, "whole-frame fact, reported once: {found:?}");
}

/// A tiny tree with no area is a plausible splash or empty state, not a bug.
#[test]
fn a_trivial_tree_is_not_reported() {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![widgets::text("x").id("x")]).id("root")
    })
    .run_headless(Size::new(400.0, 300.0));
    h.set_stylesheet("#root, #x { width: 0px; height: 0px; }");
    h.pump();
    let found: Vec<String> = h
        .lint()
        .into_iter()
        .filter(|d| d.code == "W0114")
        .map(|d| d.message)
        .collect();
    assert!(found.is_empty(), "too small to call a defect: {found:?}");
}
