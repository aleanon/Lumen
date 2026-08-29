//! T2: a text node whose intrinsic width nothing consumes is not shaped at
//! layout time.
//!
//! Shaping every label every frame — including the 99% of a long list that is
//! offscreen — measured as **87% of a 10 000-row frame**, and is the whole gap
//! to Qt and GTK, which shape at paint time and therefore only shape what they
//! draw.
//!
//! The optimisation is only sound where two things hold: nothing consumes the
//! node's intrinsic width, and nobody can *see* that its box now spans the
//! parent instead of hugging its glyphs. These tests hold the second half,
//! because that is the half that is easy to get wrong — the first version
//! moved a doc shot.

use kurbo::Size;
use lumen_core::Color;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, Element};

/// Build a column of the given width containing `child`, plus a plain label
/// after it, and report both their widths.
fn widths(width: Dim, child: Element) -> (f64, f64) {
    let mut h = App::new(move |_cx| {
        let mut col: Element = widgets::column(vec![
            child.clone().id("subject"),
            widgets::text("plain").id("plain"),
        ]);
        col.style.width = width;
        col
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    (
        h.node_bounds_by_id("subject").expect("subject").width(),
        h.node_bounds_by_id("plain").expect("plain").width(),
    )
}

/// The baseline behaviour the optimisation introduces: under a definite-width
/// column, a plain label's box spans the parent, because that is what the
/// parent's stretch does and measuring the glyphs to discover it is wasted work.
#[test]
fn a_plain_label_under_a_definite_column_spans_it() {
    let (_, plain) = widths(Dim::px(200.0), widgets::text("x").id("subject"));
    assert_eq!(plain, 200.0, "a stretched label takes the parent's width");
}

/// …but its HEIGHT must still be exactly the shaped height, because that is
/// what the layout consumes. This is the property T1 exists to make possible,
/// and getting it approximately right would move every baseline in the corpus.
#[test]
fn the_height_is_still_exact() {
    let mut h = App::new(|_cx| {
        let mut col: Element = widgets::column(vec![widgets::text("gypq jQ").id("t")]);
        col.style.width = Dim::px(200.0);
        col
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    let deferred = h.node_bounds_by_id("t").unwrap().height();

    // The same label where nothing may be deferred — a content-sized parent
    // consumes the intrinsic width, so this one is shaped.
    let mut h2 = App::new(|_cx| widgets::column(vec![widgets::text("gypq jQ").id("t")]))
        .run_headless(Size::new(300.0, 200.0));
    h2.pump();
    let shaped = h2.node_bounds_by_id("t").unwrap().height();

    assert_eq!(
        deferred, shaped,
        "the deferred height must equal the shaped height exactly"
    );
}

/// A content-sizing parent genuinely needs the intrinsic width, so nothing is
/// deferred and the label hugs its glyphs.
#[test]
fn a_content_sized_parent_still_gets_measured_children() {
    let (_, plain) = widths(Dim::Auto, widgets::text("x").id("subject"));
    assert!(
        plain > 0.0 && plain < 100.0,
        "under an Auto-width column the label must still shrink-wrap, or the \
         column has nothing to size itself from: got {plain}"
    );
}

/// A box someone can SEE must keep hugging its glyphs: a background that
/// suddenly spans the row is a rendering change, not an optimisation.
#[test]
fn a_filled_label_keeps_its_own_width() {
    let mut lab: Element = widgets::text("x").id("subject");
    lab.background = Some(Color::srgb8(0xff, 0x00, 0x00, 0xff));
    let (subject, _) = widths(Dim::px(200.0), lab);
    assert!(
        subject < 200.0,
        "a label with a background must not span the parent: got {subject}"
    );
}

/// Centred text positions itself inside its box, so the box's width is
/// observable and must not change.
#[test]
fn centred_text_keeps_its_own_width() {
    let mut lab: Element = widgets::text("x").id("subject");
    if let Some(ts) = lab.text_style_mut() {
        ts.align = lumen_text::TextAlign::Center;
    }
    let (subject, _) = widths(Dim::px(200.0), lab);
    assert!(
        subject < 200.0,
        "centred text must keep the box it is centred in: got {subject}"
    );
}

/// Wrapping needs the shaped line breaks, so a wrapped label is never deferred
/// and still wraps to more than one line.
#[test]
fn a_wrapped_label_still_wraps() {
    let mut h = App::new(|_cx| {
        let mut lab: Element = widgets::text("the quick brown fox jumps over the lazy dog").id("w");
        lab.style.width = Dim::px(80.0);
        let mut col: Element = widgets::column(vec![lab]);
        col.style.width = Dim::px(200.0);
        col
    })
    .run_headless(Size::new(300.0, 300.0));
    h.pump();
    let b = h.node_bounds_by_id("w").expect("wrapped label");
    assert!(
        b.height() > 40.0,
        "an 80px-wide paragraph must wrap to several lines: {b:?}"
    );
}
