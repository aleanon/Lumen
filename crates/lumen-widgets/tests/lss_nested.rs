//! B.1 (docs/plan-remediation-2026-07.md): nested `&` rules apply (they were
//! parsed and dropped), and descendant/`>` combinators match against the real
//! ancestor chain — before B.1 only the rightmost compound was checked, so
//! `dialog button` matched every button in the app.

use kurbo::Size;
use lumen_core::events::{Event, PointerEvent};
use lumen_widgets::{center, col, widgets, App};

fn bg_of(styles: &serde_json::Value) -> Option<String> {
    styles
        .get("background")?
        .get("value")?
        .as_str()
        .map(str::to_string)
}

#[test]
fn nested_state_rule_applies() {
    let mut h = App::new(|_cx| col![widgets::button("Hi", |_| {}).id("b")])
        .stylesheet("#b { background: #00ff00ff; &:hovered { background: #ff0000ff; } }")
        .run_headless(Size::new(300.0, 200.0));
    h.pump();
    assert_eq!(bg_of(&h.get_styles("#b")).as_deref(), Some("#00ff00ff"));
    let p = center(h.node_bounds_by_id("b").unwrap());
    h.inject(Event::PointerMove(PointerEvent::at(p)));
    h.pump();
    assert_eq!(
        bg_of(&h.get_styles("#b")).as_deref(),
        Some("#ff0000ff"),
        "nested &:hovered flattened into #b:hovered"
    );
    h.assert_view_coherent();
}

#[test]
fn descendant_rules_scope_to_real_ancestors() {
    let mut h = App::new(|_cx| {
        col![
            col![widgets::button("in", |_| {}).class("btn").id("inside")].class("card"),
            widgets::button("out", |_| {}).class("btn").id("outside"),
        ]
    })
    .stylesheet(".card .btn { background: #112233ff; }")
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    assert_eq!(
        bg_of(&h.get_styles("#inside")).as_deref(),
        Some("#112233ff"),
        "descendant inside the card styled"
    );
    assert_eq!(
        bg_of(&h.get_styles("#outside")),
        None,
        "the over-match is fixed: a .btn outside .card is untouched"
    );
    h.assert_view_coherent();
}

#[test]
fn nested_child_combinator_is_direct_only() {
    let mut h = App::new(|_cx| {
        col![
            widgets::button("direct", |_| {}).class("btn").id("direct"),
            col![widgets::button("deep", |_| {}).class("btn").id("deep")],
        ]
        .id("card")
    })
    .stylesheet("#card { & > .btn { background: #445566ff; } }")
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    assert_eq!(
        bg_of(&h.get_styles("#direct")).as_deref(),
        Some("#445566ff"),
        "direct child matched by the nested `& >` rule"
    );
    assert_eq!(
        bg_of(&h.get_styles("#deep")),
        None,
        "grandchild not matched by `>`"
    );
    h.assert_view_coherent();
}
