//! O2.2: overflow on all four edges (W0103) and off-viewport nodes (W0112).
//!
//! `check_overflow` tested only the right and bottom edges, so a node at
//! `x: -400` sat entirely off the left of its parent and raised nothing — the
//! direction a human notices *fastest*, because the content is missing rather
//! than merely cut off. And nothing anywhere checked the window: a node can sit
//! correctly inside a parent that is itself off-canvas, which W0103 is happy
//! with by construction.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};

fn findings(lss: &str, code: &str) -> Vec<String> {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![widgets::button("Save", |_| {}).id("save")]).id("root")
    })
    .run_headless(Size::new(400.0, 200.0));
    h.set_stylesheet(lss);
    h.pump();
    h.lint()
        .into_iter()
        .filter(|d| d.code == code)
        .map(|d| d.message)
        .collect()
}

#[test]
fn overflow_past_the_left_edge_is_caught() {
    let found = findings(
        "#root { width: 400px; } #save { margin-left: -300px; width: 100px; }",
        "W0103",
    );
    assert!(
        !found.is_empty(),
        "a child off the LEFT of its parent overflows it just as much as one \
         off the right: {found:?}"
    );
    assert!(
        found[0].contains("left"),
        "the message must name the edge, or the author looks at the wrong side \
         of the box: {}",
        found[0]
    );
}

#[test]
fn overflow_past_the_right_edge_still_works() {
    let found = findings("#root { width: 400px; } #save { width: 900px; }", "W0103");
    assert!(!found.is_empty(), "the original case must not regress");
    assert!(found[0].contains("right"), "edge named: {}", found[0]);
}

#[test]
fn a_node_entirely_outside_the_window_is_reported() {
    let found = findings(
        "#save { margin-left: 5000px; width: 80px; height: 30px; }",
        "W0112",
    );
    assert_eq!(
        found.len(),
        1,
        "laid out, in the tree, and nowhere on screen: {found:?}"
    );
    assert!(found[0].contains("#save"), "names the node: {}", found[0]);
}

#[test]
fn an_on_screen_node_is_not_reported() {
    assert!(
        findings("#root { width: 400px; }", "W0112").is_empty(),
        "a normal layout must stay silent"
    );
}

/// Scrolled out of view is what a scroll container is *for*. Reporting it would
/// fire on every long list and get the whole check ignored.
#[test]
fn content_scrolled_out_of_view_is_not_reported() {
    let mut h = App::new(|cx: &mut BuildCx| -> Element {
        let rows: Vec<Element> = (0..80)
            .map(|i| widgets::button(format!("row {i}"), |_| {}).id(format!("r{i}")))
            .collect();
        lumen_widgets::Scrollable::new(cx, "list", 200.0, 2400.0, rows).into()
    })
    .run_headless(Size::new(400.0, 200.0));
    h.pump();
    let found: Vec<String> = h
        .lint()
        .into_iter()
        .filter(|d| d.code == "W0112")
        .map(|d| d.message)
        .collect();
    assert!(
        found.is_empty(),
        "rows below the fold are not off-screen defects: {found:?}"
    );
}
