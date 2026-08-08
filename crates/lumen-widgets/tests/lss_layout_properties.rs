//! PROP1: the mechanical `.lss` layout properties must actually change layout.
//!
//! Twelve of the 41 parse-only properties were mechanical — `LayoutStyle` and
//! taffy already implemented every one of them, and `apply()` simply never read
//! the parsed value into the existing field. So the declaration parsed clean
//! and did nothing.
//!
//! These tests assert the *effect*, not the plumbing: a stylesheet is applied
//! and the resulting BOUNDS are checked. A test that only verified the field
//! was set would pass just as happily if the bridge to `LayoutStyle` were
//! missing — which is precisely how these got shipped unimplemented.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};

fn bounds_of(lss: &str, id: &str) -> kurbo::Rect {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::row(vec![widgets::text("a").id("a"), widgets::text("b").id("b")]).id("root")
    })
    .run_headless(Size::new(400.0, 200.0));
    h.set_stylesheet(lss);
    h.pump();
    h.node_bounds_by_id(id)
        .unwrap_or_else(|| panic!("`{id}` should be laid out"))
}

#[test]
fn justify_content_moves_children_along_the_main_axis() {
    let start = bounds_of("#root { width: 400px; justify-content: start; }", "a");
    let end = bounds_of("#root { width: 400px; justify-content: end; }", "a");
    assert!(
        end.x0 > start.x0,
        "justify-content: end must push content right (start x0={}, end x0={})",
        start.x0,
        end.x0
    );

    let center = bounds_of("#root { width: 400px; justify-content: center; }", "a");
    assert!(
        center.x0 > start.x0 && center.x0 < end.x0,
        "center must fall between start and end, got {}",
        center.x0
    );
}

#[test]
fn flex_start_and_start_are_equivalent() {
    // CSS authors write both spellings; accepting only one would be a silent
    // no-op for the other, which is the defect class this workstream removes.
    let a = bounds_of("#root { width: 400px; justify-content: end; }", "a");
    let b = bounds_of("#root { width: 400px; justify-content: flex-end; }", "a");
    assert_eq!(a.x0, b.x0, "`end` and `flex-end` must behave identically");
}

#[test]
fn min_and_max_width_constrain_the_box() {
    let wide = bounds_of("#a { min-width: 200px; }", "a");
    assert!(
        wide.width() >= 200.0,
        "min-width must widen a small text node, got {}",
        wide.width()
    );

    let capped = bounds_of("#a { width: 300px; max-width: 50px; }", "a");
    assert!(
        capped.width() <= 50.0,
        "max-width must cap an over-wide box, got {}",
        capped.width()
    );
}

#[test]
fn flex_grow_distributes_free_space() {
    let plain = bounds_of("#root { width: 400px; }", "a");
    let grown = bounds_of("#root { width: 400px; } #a { flex-grow: 1; }", "a");
    assert!(
        grown.width() > plain.width(),
        "flex-grow must claim free space (plain={}, grown={})",
        plain.width(),
        grown.width()
    );
}

#[test]
fn column_gap_separates_children() {
    let tight = bounds_of("#root { width: 400px; column-gap: 0px; }", "b");
    let loose = bounds_of("#root { width: 400px; column-gap: 40px; }", "b");
    assert!(
        loose.x0 >= tight.x0 + 39.0,
        "column-gap must push the second child right (tight={}, loose={})",
        tight.x0,
        loose.x0
    );
}

#[test]
fn per_axis_gap_overrides_the_shorthand() {
    // Source-order intuition: the longhand wins, like padding-left over padding.
    let both = bounds_of("#root { width: 400px; gap: 10px; column-gap: 50px; }", "b");
    let only = bounds_of("#root { width: 400px; gap: 10px; }", "b");
    assert!(
        both.x0 > only.x0,
        "column-gap must override the gap shorthand ({} vs {})",
        both.x0,
        only.x0
    );
}
