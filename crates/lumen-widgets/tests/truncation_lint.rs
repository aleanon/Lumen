//! O3.1: truncated text is knowable (W0403).
//!
//! `text-overflow: ellipsis` paints a truncated string while the semantic tree
//! keeps the full one. That split is deliberate and right — the existing test
//! `text_overflow_truncates_the_paint_but_not_the_semantics` calls it "the whole
//! feature", and truncating the tree would make `ui.getTree` report
//! `"Some long lab…"`, corrupting the observability surface to fix a visual one.
//!
//! But it left an agent confidently wrong: the screen reads `Quarterly rev…`,
//! the tree reads the full label, `assertText` passes, and the real bug — the
//! column is too narrow — is invisible. This adds the missing third option:
//! keep the label full, and report the split.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};

fn app(lss: &str) -> lumen_widgets::Headless {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![widgets::text("Quarterly revenue by region").id("a")]).id("root")
    })
    .run_headless(Size::new(400.0, 200.0));
    h.set_stylesheet(lss);
    h.pump();
    h
}

const NARROW: &str = "#a { width: 90px; text-wrap: nowrap; text-overflow: ellipsis; }";
const WIDE: &str = "#a { width: 400px; text-wrap: nowrap; text-overflow: ellipsis; }";

#[test]
fn truncated_text_is_reported() {
    let mut h = app(NARROW);
    let found: Vec<String> = h
        .lint()
        .into_iter()
        .filter(|d| d.code == "W0403")
        .map(|d| d.message)
        .collect();
    assert_eq!(found.len(), 1, "the truncation is knowable now: {found:?}");
    assert!(
        found[0].contains('…'),
        "the message must show what is actually painted: {}",
        found[0]
    );
}

/// The deliberate design is preserved: the label stays FULL, so a11y and
/// selectors keep working. Only the report is new.
#[test]
fn the_semantic_label_is_still_the_full_string() {
    let h = app(NARROW);
    let tree = h.semantics_json();
    let s = tree.to_string();
    assert!(
        s.contains("Quarterly revenue by region"),
        "the tree must NOT be truncated -- that is the whole feature: {s}"
    );
}

#[test]
fn text_that_fits_is_not_reported() {
    let mut h = app(WIDE);
    let found: Vec<String> = h
        .lint()
        .into_iter()
        .filter(|d| d.code == "W0403")
        .map(|d| d.message)
        .collect();
    assert!(found.is_empty(), "nothing was truncated: {found:?}");
}

/// The query side: an agent that already suspects a node can ask directly.
#[test]
fn get_layout_reports_the_painted_string() {
    let h = app(NARROW);
    let painted = h.node_painted_text("#a").expect("truncated");
    assert!(
        painted.ends_with('…'),
        "painted with an ellipsis: {painted}"
    );
    assert!(
        painted.len() < "Quarterly revenue by region".len(),
        "shorter than the label: {painted}"
    );
    assert!(
        h.node_painted_text("#root").is_none(),
        "a node that is not truncated reports nothing"
    );
}
