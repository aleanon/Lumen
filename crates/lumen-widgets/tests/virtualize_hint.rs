//! VL1: a scroll container laying out hundreds of children directly should say
//! so.
//!
//! `Scrollable` materializes every child every frame — its own module doc says
//! "for very long lists, virtualize — this lays out all children" — while
//! `VirtualList` renders only the visible window and is flat in item count
//! (1.15 ms for 1M rows). Both have existed for a long time; the gap is
//! discoverability. The cheap-looking widget is the obvious one, and the
//! scalable one is the widget you have to already know about.
//!
//! The 2026-08 review called the unvirtualized default disqualifying for the
//! "peak performance" claim on any list-driven app.

use kurbo::Size;
use lumen_core::codes;
use lumen_widgets::{widgets, App, BuildCx, Element, Scrollable};

fn rows(n: usize) -> Vec<Element> {
    (0..n).map(|i| widgets::text(format!("row {i}"))).collect()
}

fn lint_of(n: usize) -> Vec<lumen_core::Diagnostic> {
    let mut h = App::new(move |cx: &mut BuildCx| -> Element {
        Scrollable::new(cx, "list", 600.0, (n as f64) * 20.0, rows(n))
            .id("list")
            .into()
    })
    .run_headless(Size::new(400.0, 600.0));
    h.pump();
    h.lint()
}

#[test]
fn a_long_unvirtualized_list_is_flagged() {
    let diags = lint_of(500);
    let hit = diags.iter().find(|d| d.code == codes::W0108);
    let hit = hit.unwrap_or_else(|| panic!("expected W0108; got {diags:?}"));
    assert!(
        hit.message.contains("VirtualList"),
        "the hint must name the alternative, not just the problem: {}",
        hit.message
    );
    // Report a count, without pinning the exact number: the lint walks the
    // raw tree, which includes structural wrappers the author never wrote, so
    // the figure is "nodes laid out" rather than "rows you typed".
    let n: usize = hit
        .message
        .split_whitespace()
        .find_map(|w| w.parse().ok())
        .expect("the hint must say how much it saw");
    assert!(
        n >= 500,
        "expected at least the 500 rows to be counted, got {n}: {}",
        hit.message
    );
}

#[test]
fn an_ordinary_list_is_not_nagged() {
    // A settings page or nav list has tens of rows. Warning about those would
    // train everyone to ignore the diagnostic.
    let diags = lint_of(20);
    assert!(
        !diags.iter().any(|d| d.code == codes::W0108),
        "a short list must not be flagged; got {diags:?}"
    );
}
