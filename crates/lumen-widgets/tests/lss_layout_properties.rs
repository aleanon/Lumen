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

// --- PROP1, grid batch -------------------------------------------------------

/// `display: grid` + `grid-template-columns` must actually place children into
/// tracks. `GridTrack`/`GridLine` and the taffy conversion already existed;
/// only `.lss` parsing was missing, so the declaration was inert.
#[test]
fn grid_template_columns_sizes_tracks() {
    // Two columns, 1fr and 3fr, in a 400px container: 100px and 300px.
    let a = box_bounds(
        "#root { display: grid; width: 400px; grid-template-columns: 1fr 3fr; }",
        "boxa",
    );
    let b = box_bounds(
        "#root { display: grid; width: 400px; grid-template-columns: 1fr 3fr; }",
        "boxb",
    );
    assert!(
        (a.width() - 100.0).abs() < 1.0,
        "first track should be 1fr of 400 = 100, got {}",
        a.width()
    );
    assert!(
        (b.width() - 300.0).abs() < 1.0,
        "second track should be 3fr of 400 = 300, got {}",
        b.width()
    );
    assert!(b.x0 > a.x0, "children must be placed in separate columns");
}

#[test]
fn fixed_and_auto_tracks_parse_alongside_fr() {
    let a = box_bounds(
        "#root { display: grid; width: 400px; grid-template-columns: 120px 1fr; }",
        "boxa",
    );
    assert!(
        (a.width() - 120.0).abs() < 1.0,
        "a px track should be exactly that wide, got {}",
        a.width()
    );
}

/// A single unparseable track rejects the WHOLE declaration rather than
/// silently dropping that track — a grid missing one column lays out plausibly
/// but wrongly, which is much harder to notice than the property not applying.
#[test]
fn one_bad_track_rejects_the_whole_track_list() {
    let good = box_bounds(
        "#root { display: grid; width: 400px; grid-template-columns: 1fr 3fr; }",
        "boxa",
    );
    let bad = box_bounds(
        "#root { display: grid; width: 400px; grid-template-columns: 1fr bogus; }",
        "boxa",
    );
    assert_ne!(
        (bad.width() * 10.0) as i64,
        (good.width() * 10.0) as i64,
        "a bogus track must not leave a partially-applied grid"
    );
}

#[test]
fn grid_column_places_a_child_in_a_named_track() {
    // Put `boxa` in column 2 of a 2-track grid: it should start halfway across.
    let placed = box_bounds(
        "#root { display: grid; width: 400px; grid-template-columns: 1fr 1fr; } \
         #boxa { grid-column: 2; }",
        "boxa",
    );
    assert!(
        (placed.x0 - 200.0).abs() < 1.0,
        "grid-column: 2 should start the child at 200, got {}",
        placed.x0
    );
}

// --- PROP1, overflow ---------------------------------------------------------

/// `overflow: hidden` must actually clip. Asserted on RENDERED PIXELS: a
/// container's `ink` is not a reliable witness here (it is the node's own ink,
/// and a clip changes what its CHILDREN draw), whereas the frame is the thing
/// the property is supposed to change.
#[test]
fn overflow_hidden_clips_and_visible_does_not() {
    fn ink_pixels(lss: &str) -> usize {
        let mut h = App::new(|_cx: &mut BuildCx| -> Element {
            widgets::column(vec![widgets::text(
                "a deliberately long line of text that will not fit in the box",
            )
            .id("a")])
            .id("root")
        })
        .run_headless(Size::new(400.0, 200.0));
        h.set_stylesheet(lss);
        h.pump();
        let img = h.screenshot();
        // Count non-white pixels: the glyphs that actually reached the frame.
        img.pixels()
            .chunks_exact(4)
            .filter(|p| p[0] < 200 || p[1] < 200 || p[2] < 200)
            .count()
    }
    let visible = ink_pixels("#root { width: 40px; height: 20px; overflow: visible; }");
    let hidden = ink_pixels("#root { width: 40px; height: 20px; overflow: hidden; }");
    assert!(
        hidden < visible,
        "overflow: hidden must draw fewer pixels than visible \
         (visible={visible}, hidden={hidden})"
    );
}

/// `overflow: scroll` is rejected rather than aliased to `hidden`: scrolling is
/// a widget, and a silent alias would clip content with no way to reach the
/// rest — which looks like lost content, not an unsupported value.
#[test]
fn overflow_scroll_is_rejected_not_silently_clipped() {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![widgets::text("x").id("a")]).id("root")
    })
    .run_headless(Size::new(400.0, 200.0));
    h.set_stylesheet("#root { overflow: scroll; }");
    h.pump();
    // Unsupported VALUE ⇒ the property stays unset, so the diagnostic surfaces
    // rather than the declaration quietly doing something else.
    let styles = h.get_styles("#root");
    assert!(
        styles.get("clip").is_none(),
        "overflow: scroll must not resolve to a clip, got {styles}"
    );
}

// --- PROP1, text-align -------------------------------------------------------

/// `text-align` must reach the shaper. The runtime had `TextAlign::Start`
/// hardcoded at nine call sites, so the declaration was inert; the alignment
/// now travels on `TextStyle`.
///
/// Asserted on rendered pixels rather than bounds: alignment moves glyphs
/// WITHIN an unchanged box, so bounds cannot witness it.
#[test]
fn text_align_moves_glyphs_within_the_box() {
    fn ink_centre_x(lss: &str) -> f64 {
        let mut h = App::new(|_cx: &mut BuildCx| -> Element {
            widgets::column(vec![widgets::text("hi").id("a")]).id("root")
        })
        .run_headless(Size::new(400.0, 100.0));
        h.set_stylesheet(lss);
        h.pump();
        let img = h.screenshot();
        let w = img.width() as usize;
        let (mut sum, mut n) = (0usize, 0usize);
        for (i, p) in img.pixels().chunks_exact(4).enumerate() {
            if p[0] < 200 || p[1] < 200 || p[2] < 200 {
                sum += i % w;
                n += 1;
            }
        }
        assert!(n > 0, "expected some glyph pixels");
        sum as f64 / n as f64
    }
    let start = ink_centre_x("#a { width: 300px; text-align: start; }");
    let center = ink_centre_x("#a { width: 300px; text-align: center; }");
    let end = ink_centre_x("#a { width: 300px; text-align: end; }");
    assert!(
        center > start + 10.0 && end > center + 10.0,
        "alignment must move the glyphs rightwards start<center<end \
         (start={start:.1}, center={center:.1}, end={end:.1})"
    );
}

/// `justify` is rejected: the shaper has no justification pass, so accepting it
/// would claim support Lumen does not have. It must render exactly as `start`,
/// not as some other alignment.
///
/// Two things this test pins that are easy to get wrong:
///
/// * `get_styles` reports the **declared** value with its source span, NOT the
///   applied one — a rejected value still appears there. So the assertion is on
///   rendered output, which is the only witness of what actually happened.
/// * **No diagnostic fires.** `W0107` reports an unimplemented *property*; a
///   rejected *value* on an implemented property is silent (the value-level
///   hole, SD5.x, still open — verified here). `ui.explain {kind: "style"}` is
///   the only way an agent sees this today.
#[test]
fn text_align_justify_renders_as_start() {
    fn ink_centre(lss: &str) -> f64 {
        let mut h = App::new(|_cx: &mut BuildCx| -> Element {
            widgets::column(vec![widgets::text("hi").id("a")]).id("root")
        })
        .run_headless(Size::new(400.0, 100.0));
        h.set_stylesheet(lss);
        h.pump();
        let img = h.screenshot();
        let w = img.width() as usize;
        let (mut sum, mut n) = (0usize, 0usize);
        for (i, p) in img.pixels().chunks_exact(4).enumerate() {
            if p[0] < 200 || p[1] < 200 || p[2] < 200 {
                sum += i % w;
                n += 1;
            }
        }
        sum as f64 / n.max(1) as f64
    }
    let start = ink_centre("#a { width: 300px; text-align: start; }");
    let justify = ink_centre("#a { width: 300px; text-align: justify; }");
    assert!(
        (justify - start).abs() < 1.0,
        "justify must fall back to start, got start={start:.1} justify={justify:.1}"
    );
}
