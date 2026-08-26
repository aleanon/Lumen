//! WT-EXP P1 — does text measurement survive without an `Element` to mutate?
//!
//! `build_node` shapes a text leaf and writes a fixed size onto the style before
//! taffy sees it, reconciling three inputs that arrive at different times: the
//! widget's own width, the cascade's `text-wrap`, and the content. In the sink
//! they meet at `end()` instead.
//!
//! These tests hold the two rules `build_node` documents as hard-won — an axis
//! the author fixed is never overwritten by a measurement, and a percentage
//! width cannot feed the wrap width — plus agreement between the two paths.

use lumen_core::semantics::Role;
use lumen_layout::{Dim, LayoutStyle};
use lumen_widgets::direct::{lower_element, StyleEnv, TreeSink, VisualState};
use lumen_widgets::{Element, Label};

fn sink() -> TreeSink {
    TreeSink::new().with_text(lumen_text::TextEngine::new())
}

fn styled_sink(src: &str) -> TreeSink {
    TreeSink::new()
        .with_text(lumen_text::TextEngine::new())
        .with_styles(
            StyleEnv::from_source(src).expect("parses"),
            VisualState::default(),
        )
}

/// Lower a `Label` through the direct path and return its final layout style.
fn direct_label(s: &mut TreeSink, label: Label) -> LayoutStyle {
    use lumen_widgets::direct::Direct;
    let (n, _) = label.lower(s, None);
    s.meta.layout_style(n).clone()
}

/// Lower the same `Label` through the Element path.
fn element_label(s: &mut TreeSink, label: Label) -> LayoutStyle {
    let el: Element = label.into();
    let (n, _) = lower_element(el, s, None);
    s.meta.layout_style(n).clone()
}

#[test]
fn an_auto_width_label_is_sized_to_its_glyphs() {
    let mut s = sink();
    let st = direct_label(&mut s, Label::new("hello world"));
    match st.width {
        Dim::Px(w) => assert!(w > 20.0, "measured a real width, got {w}"),
        other => panic!("auto width should have been measured, got {other:?}"),
    }
    match st.height {
        Dim::Px(h) => assert!(h > 5.0, "measured a real height, got {h}"),
        other => panic!("auto height should have been measured, got {other:?}"),
    }
}

#[test]
fn both_paths_measure_the_same_box() {
    // The property that matters: removing the staging element must not change
    // a single laid-out pixel.
    for text in ["hi", "hello world", "a much longer run of text to shape"] {
        let mut a = sink();
        let mut b = sink();
        let da = direct_label(&mut a, Label::new(text));
        let eb = element_label(&mut b, Label::new(text));
        assert_eq!(da.width, eb.width, "width differs for {text:?}");
        assert_eq!(da.height, eb.height, "height differs for {text:?}");
    }
}

#[test]
fn an_explicit_width_is_never_overwritten_by_measurement() {
    // `build_node` documents this as costing real bugs: a fixed width must
    // survive, and only the wrapped height comes from the block.
    let mut s = sink();
    let st = direct_label(&mut s, Label::new("a long paragraph that will wrap").width(120.0));
    assert_eq!(st.width, Dim::px(120.0), "the author's width survived");
    match st.height {
        Dim::Px(h) => assert!(h > 5.0, "height came from the wrapped block"),
        other => panic!("height should be measured, got {other:?}"),
    }
}

#[test]
fn wrapping_makes_a_paragraph_taller_than_one_line() {
    let mut s = sink();
    let one = direct_label(&mut s, Label::new("word ".repeat(20)).width(600.0));
    let mut s2 = sink();
    let many = direct_label(&mut s2, Label::new("word ".repeat(20)).width(80.0));
    let (h1, h2) = (
        match one.height {
            Dim::Px(v) => v,
            _ => panic!(),
        },
        match many.height {
            Dim::Px(v) => v,
            _ => panic!(),
        },
    );
    assert!(
        h2 > h1,
        "a narrower box wraps to more lines: {h1} vs {h2}"
    );
}

#[test]
fn nowrap_keeps_the_box_width_but_shapes_one_line() {
    // PROP1: `text-wrap: nowrap` keeps the explicit width for the BOX and
    // shapes unwrapped, so the run overflows on one line instead of folding.
    // The cascade sets it, so this only works if the sheet reaches measurement
    // — which is the whole question this file exists to answer.
    let mut wrapped = styled_sink("");
    let mut nowrapped = styled_sink("text { text-wrap: nowrap; }");
    let w = direct_label(&mut wrapped, Label::new("word ".repeat(20)).width(80.0));
    let n = direct_label(&mut nowrapped, Label::new("word ".repeat(20)).width(80.0));
    assert_eq!(n.width, Dim::px(80.0), "the box keeps its width");
    let (hw, hn) = (
        match w.height {
            Dim::Px(v) => v,
            _ => panic!(),
        },
        match n.height {
            Dim::Px(v) => v,
            _ => panic!(),
        },
    );
    assert!(
        hn < hw,
        "nowrap is one line, so shorter than the wrapped version: {hn} vs {hw}"
    );
}

#[test]
fn a_percentage_width_does_not_feed_the_wrap_width() {
    // The containing block is not resolved until layout runs, which is after
    // measurement — so a percentage-width label lays out as one unwrapped line.
    let mut s = sink();
    use lumen_widgets::direct::Direct;
    let (n, _) = Label::new("word ".repeat(20))
        .lower(&mut s, None);
    let auto_h = match s.meta.layout_style(n).height {
        Dim::Px(v) => v,
        _ => panic!(),
    };

    let mut s2 = sink();
    let pct = s2.node(None, Role::Text).text("word ".repeat(20), Default::default());
    let node = pct.resolve();
    let idx = node.index();
    node.end(
        &LayoutStyle {
            width: Dim::pct(1.0),
            ..LayoutStyle::default()
        },
        &[],
        false,
    );
    let pct_h = match s2.meta.layout_style(idx).height {
        Dim::Px(v) => v,
        _ => panic!(),
    };
    assert_eq!(
        pct_h, auto_h,
        "a percentage width shapes one unwrapped line, same as auto"
    );
    assert_eq!(
        s2.meta.layout_style(idx).width,
        Dim::pct(1.0),
        "and the percentage survives for the box"
    );
}
