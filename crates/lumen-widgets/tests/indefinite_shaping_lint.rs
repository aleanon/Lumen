//! W0404: text shaped during layout because a container sizes to its content.
//!
//! Lumen defers a single unwrapped label's shaping to paint when its parent
//! assigns its width — so a long list costs the rows it draws, not the rows it
//! has. A content-sizing container defeats that for its whole subtree, because
//! it genuinely needs each child's glyph width to size itself.
//!
//! That is a legitimate thing to write, and on a small container it is free.
//! On a list it was measured at 87% of the frame — and it is invisible: the
//! layout is right, the tree looks healthy, the app is just slower. So it is
//! reported rather than left to profiling.

use kurbo::Size;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, Element};

fn findings(width: Dim, rows: usize) -> Vec<lumen_core::Diagnostic> {
    let mut h = App::new(move |_cx| {
        let mut col: Element = widgets::column(
            (0..rows)
                .map(|i| widgets::text(format!("row {i}")))
                .collect::<Vec<_>>(),
        );
        col.style.width = width;
        col
    })
    .run_headless(Size::new(400.0, 600.0));
    h.pump();
    h.lint()
        .into_iter()
        .filter(|d| d.code == lumen_core::codes::W0404)
        .collect()
}

#[test]
fn a_content_sized_list_is_reported() {
    let f = findings(Dim::Auto, 200);
    assert_eq!(f.len(), 1, "reported once, not once per node");
    assert!(
        f[0].message.contains("200"),
        "the finding carries the count, so the cost is quantified: {}",
        f[0].message
    );
    assert!(
        f[0].message.contains("width"),
        "and names the fix: {}",
        f[0].message
    );
}

/// The same list under a definite width shapes nothing at layout time, so there
/// is nothing to report. This is the property the lint exists to point at.
#[test]
fn a_definite_container_reports_nothing() {
    assert!(
        findings(Dim::px(300.0), 200).is_empty(),
        "a definite container defers its labels' shaping, so there is no finding"
    );
}

/// A shrink-to-fit container is a normal thing to write — a menu, a tooltip, a
/// badge. Reporting those would be noise, so the lint has a floor.
#[test]
fn a_small_shrink_to_fit_container_is_not_noise() {
    assert!(
        findings(Dim::Auto, 4).is_empty(),
        "four labels in a shrink-to-fit box is ordinary, not a finding"
    );
}
