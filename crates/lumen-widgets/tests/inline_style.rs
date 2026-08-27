//! B.6b (docs/plan-remediation-2026-07.md): the typed inline style —
//! `Origin::Inline` in the 04 §2 cascade. `.css(Style)` beats stylesheet
//! declarations unless they are `!important`; inline layout properties reach
//! taffy; `ui.getStyles` reports `source: "inline"`.

use kurbo::{Point, Size};
use lumen_core::events::{Event, PointerEvent};
use lumen_core::Color;
use lumen_layout::Dim;
use lumen_style::Style;
use lumen_widgets::{center, col, widgets, App, Element};

fn red() -> Color {
    Color::from_hex("#ff0000ff").unwrap()
}

fn boxed(id: &str) -> Element {
    let mut e: Element = widgets::button("", |_| {}).id(id);
    e.style.width = Dim::px(100.0);
    e
}

#[test]
fn inline_beats_the_stylesheet_but_not_important() {
    let sheet = "#a { background: #0000ffff; } \
                 #b { background: #0000ffff !important; }";
    let mut h = App::new(|_cx| {
        col![
            boxed("a").css(Style::new().background(red())),
            boxed("b").css(Style::new().background(red()))
        ]
    })
    .stylesheet(sheet)
    .run_headless(Size::new(300.0, 200.0));
    h.pump();

    let bg = |h: &lumen_widgets::Headless, id: &str| {
        h.get_styles(id)["background"]["value"]
            .as_str()
            .map(str::to_string)
    };
    assert_eq!(
        bg(&h, "#a").as_deref(),
        Some("#ff0000ff"),
        "inline beats the plain sheet declaration"
    );
    assert_eq!(
        h.get_styles("#a")["background"]["source"],
        "inline",
        "computed map reports the inline origin"
    );
    assert_eq!(
        bg(&h, "#b").as_deref(),
        Some("#0000ffff"),
        "!important sheet declaration beats inline (04 §2)"
    );
    h.assert_view_coherent();
}

#[test]
fn inline_layout_properties_reach_taffy() {
    let mut h = App::new(|_cx| col![boxed("w").css(Style::new().width(240.0))])
        .run_headless(Size::new(400.0, 200.0));
    h.pump();
    let b = h.node_bounds_by_id("w").unwrap();
    assert!(
        (b.width() - 240.0).abs() < 1.0,
        "inline width overrides the element's LayoutStyle: {b:?}"
    );
}

#[test]
fn inline_style_survives_the_restyle_only_hover_path() {
    // Hovering flips :hovered rules via the A.5 restyle path — the retained
    // inline style must re-merge (it lives in NodeMeta, not the sheet).
    let sheet = "button:hovered { color: #00ff00ff; }";
    let mut h = App::new(|_cx| col![boxed("k").css(Style::new().background(red()))])
        .stylesheet(sheet)
        .run_headless(Size::new(300.0, 200.0));
    h.pump();

    let p = center(h.node_bounds_by_id("k").unwrap());
    h.inject(Event::PointerMove(PointerEvent::at(p)));
    h.pump();
    assert_eq!(
        h.get_styles("#k")["background"]["value"].as_str(),
        Some("#ff0000ff"),
        "inline background persists through a restyle"
    );
    // And back off.
    h.inject(Event::PointerMove(PointerEvent::at(Point::new(1.0, 1.0))));
    h.pump();
    assert_eq!(
        h.get_styles("#k")["background"]["value"].as_str(),
        Some("#ff0000ff")
    );
    h.assert_view_coherent();
}

/// O0.6: the computed-value map is SHARED between nodes that resolve alike
/// (an `Rc` out of the A.5b memo instead of a per-node deep clone). An inline
/// style is the one thing that writes into it after the fact, so it must fork
/// its own copy first — otherwise the write lands in the map every sibling
/// with the same cascade identity is holding, and `ui.getStyles` reports one
/// node's inline declaration on all of them.
///
/// The two rows here are deliberately id-less and identically classed, which
/// is exactly what makes their memo key equal (the key hashes id, classes,
/// states and type — never the inline style). `:nth()` addresses them apart.
#[test]
fn an_inline_style_does_not_leak_through_the_shared_computed_map() {
    let sheet = ".row { background: #0000ffff; }";
    let mut h = App::new(|_cx| {
        let mut a: Element = widgets::button("", |_| {}).class("row");
        a.style.width = Dim::px(100.0);
        let mut b: Element = widgets::button("", |_| {}).class("row");
        b.style.width = Dim::px(100.0);
        col![a.css(Style::new().background(red())), b]
    })
    .stylesheet(sheet)
    .run_headless(Size::new(300.0, 200.0));
    h.pump();

    // Non-vacuous: the second row must have HIT the memo entry the first one
    // filled, i.e. the two really are sharing one map.
    let (hits, _) = h.style_memo_stats();
    assert!(hits > 0, "the two rows must share one memo entry");

    let bg = |h: &lumen_widgets::Headless, sel: &str| {
        h.get_styles(sel)["background"]["value"]
            .as_str()
            .map(str::to_string)
    };
    assert_eq!(
        bg(&h, ".row:nth(1)").as_deref(),
        Some("#ff0000ff"),
        "the inline row reports its own declaration"
    );
    assert_eq!(
        bg(&h, ".row:nth(2)").as_deref(),
        Some("#0000ffff"),
        "its sibling still reports the SHEET value — the inline write must \
         not have landed in the shared map"
    );
    h.assert_view_coherent();
}
