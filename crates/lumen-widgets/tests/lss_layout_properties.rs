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

// --- PROP1, second batch -----------------------------------------------------
//
// Nine more properties whose `LayoutStyle` fields already existed. Same
// discipline as above: assert the laid-out BOUNDS, because a test that checked
// only the parsed field would pass with the bridge to `LayoutStyle` missing —
// which is exactly the defect being fixed.

#[test]
fn position_absolute_with_inset_takes_a_node_out_of_flow() {
    // In flow, `b` follows `a` horizontally.
    let flow = bounds_of("#root { width: 400px; }", "b");
    // Out of flow and pinned 50px from the container's left/top edges.
    let pinned = bounds_of(
        "#root { width: 400px; height: 200px; } \
         #b { position: absolute; inset-left: 50px; inset-top: 30px; }",
        "b",
    );
    assert_ne!(
        pinned.x0, flow.x0,
        "position: absolute must not leave the node in flow"
    );
    assert!(
        (pinned.x0 - 50.0).abs() < 1.0,
        "inset-left should pin x0 to 50, got {}",
        pinned.x0
    );
    assert!(
        (pinned.y0 - 30.0).abs() < 1.0,
        "inset-top should pin y0 to 30, got {}",
        pinned.y0
    );
}

#[test]
fn inset_shorthand_pins_all_four_sides() {
    let r = bounds_of(
        "#root { width: 400px; height: 200px; } #b { position: absolute; inset: 20px; }",
        "b",
    );
    assert!(
        (r.x0 - 20.0).abs() < 1.0 && (r.y0 - 20.0).abs() < 1.0,
        "inset: 20px should pin the top-left corner to (20, 20), got ({}, {})",
        r.x0,
        r.y0
    );
}

/// `aspect-ratio` is tested on an empty BOX, not on the text nodes the other
/// cases use: the runtime writes a measured height onto a text leaf's
/// `LayoutStyle`, and an explicit height beats the ratio. That is correct
/// behaviour, but it makes a text node useless as a subject here.
fn box_bounds(lss: &str, id: &str) -> kurbo::Rect {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::row(vec![
            widgets::column(vec![]).id("boxa"),
            widgets::column(vec![]).id("boxb"),
        ])
        .id("root")
    })
    .run_headless(Size::new(400.0, 200.0));
    h.set_stylesheet(lss);
    h.pump();
    h.node_bounds_by_id(id)
        .unwrap_or_else(|| panic!("`{id}` should be laid out"))
}

#[test]
fn aspect_ratio_derives_height_from_width() {
    let square = box_bounds("#boxa { width: 80px; aspect-ratio: 1; }", "boxa");
    assert!(
        (square.height() - 80.0).abs() < 1.0,
        "aspect-ratio: 1 at width 80 should give height 80, got {}",
        square.height()
    );
    let wide = box_bounds("#boxa { width: 80px; aspect-ratio: 2; }", "boxa");
    assert!(
        (wide.height() - 40.0).abs() < 1.0,
        "aspect-ratio: 2 at width 80 should give height 40, got {}",
        wide.height()
    );
}

/// A zero or negative ratio would collapse the node in taffy, so `as_aspect_ratio`
/// rejects it and the property stays unset — the element keeps its natural size
/// rather than vanishing.
#[test]
fn a_nonsense_aspect_ratio_is_rejected_rather_than_collapsing_the_node() {
    let natural = box_bounds("#boxa { width: 80px; height: 25px; }", "boxa");
    let zero = box_bounds(
        "#boxa { width: 80px; height: 25px; aspect-ratio: 0; }",
        "boxa",
    );
    assert!(
        zero.height() > 0.0,
        "aspect-ratio: 0 must not collapse the node"
    );
    assert!((zero.height() - natural.height()).abs() < 1.0);
}

#[test]
fn flex_basis_sets_the_main_axis_base_size() {
    let base = bounds_of("#root { width: 400px; } #a { flex-basis: 120px; }", "a");
    assert!(
        (base.width() - 120.0).abs() < 1.0,
        "flex-basis: 120px should size `a` to 120 wide, got {}",
        base.width()
    );
}

#[test]
fn align_content_moves_wrapped_lines_along_the_cross_axis() {
    // Two lines: each child is forced wider than half the container.
    let sheet = |ac: &str| {
        format!(
            "#root {{ width: 200px; height: 300px; flex-wrap: wrap; align-content: {ac}; }} \
             #a, #b {{ width: 150px; height: 40px; }}"
        )
    };
    let start = bounds_of(&sheet("start"), "b");
    let end = bounds_of(&sheet("end"), "b");
    assert!(
        end.y0 > start.y0,
        "align-content: end must push wrapped lines down (start y0={}, end y0={})",
        start.y0,
        end.y0
    );
}

// --- PROP1, typography batch -------------------------------------------------

/// `letter-spacing` must reach the text stack, i.e. change the MEASURED width.
/// Asserting the field were set would pass with the bridge missing.
#[test]
fn letter_spacing_widens_measured_text() {
    let tight = bounds_of("#a { letter-spacing: 0px; }", "a");
    let loose = bounds_of("#a { letter-spacing: 10px; }", "a");
    assert!(
        loose.width() > tight.width() + 5.0,
        "letter-spacing must widen the run (tight={}, loose={})",
        tight.width(),
        loose.width()
    );
}

/// An unregistered `font-family` falls back to the bundled face rather than
/// failing or rendering tofu — the same contract `TextStyle::family` documents.
/// The declaration must still be *applied* (it reaches `TextStyle`), which is
/// what distinguishes this from the silent-discard bug being fixed.
#[test]
fn an_unknown_font_family_falls_back_to_the_bundled_face() {
    let plain = bounds_of("#a { font-size: 16px; }", "a");
    let named = bounds_of("#a { font-size: 16px; font-family: NotInstalled; }", "a");
    assert!(
        (named.width() - plain.width()).abs() < 0.5,
        "unknown family should shape identically to the default \
         (plain={}, named={})",
        plain.width(),
        named.width()
    );
}
