//! B.6a (docs/plan-remediation-2026-07.md): the full state-selector
//! vocabulary — semantic widget states (`:checked`, `:disabled`, …) are
//! style-matchable, and the CSS-familiar aliases (`:hover`, `:focus`) work
//! alongside the canonical `:hovered`/`:focused`.

use kurbo::Size;
use lumen_core::events::{Event, PointerEvent};
use lumen_core::semantics::State as SemState;
use lumen_widgets::{center, col, widgets, App, Element};

fn bg_of(styles: &serde_json::Value) -> Option<String> {
    styles
        .get("background")?
        .get("value")?
        .as_str()
        .map(str::to_string)
}

#[test]
fn checked_state_styles_a_checkbox() {
    let mut h = App::new(|cx| col![widgets::checkbox(cx, "t", "Label").id("c")])
        .stylesheet("#c:checked { background: #22aa44ff; }")
        .run_headless(Size::new(300.0, 200.0));
    h.pump();
    assert_eq!(bg_of(&h.get_styles("#c")), None, "unchecked: no rule");
    let p = center(h.node_bounds_by_id("c").unwrap());
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
    assert_eq!(
        bg_of(&h.get_styles("#c")).as_deref(),
        Some("#22aa44ff"),
        "checked semantic state drives the rule"
    );
    h.assert_view_coherent();
}

#[test]
fn hover_alias_matches_like_hovered() {
    let mut h = App::new(|_cx| col![widgets::button("Hi", |_| {}).id("b")])
        .stylesheet("#b:hover { background: #ff0000ff; }")
        .run_headless(Size::new(300.0, 200.0));
    h.pump();
    let p = center(h.node_bounds_by_id("b").unwrap());
    h.inject(Event::PointerMove(PointerEvent::at(p)));
    h.pump();
    assert_eq!(
        bg_of(&h.get_styles("#b")).as_deref(),
        Some("#ff0000ff"),
        "the CSS-familiar :hover spelling works"
    );
    h.assert_view_coherent();
}

#[test]
fn disabled_semantic_state_is_matchable() {
    let mut h = App::new(|_cx| {
        let mut e: Element = widgets::button("Nope", |_| {}).id("d");
        e.states.push(SemState::Disabled);
        col![e]
    })
    .stylesheet("#d:disabled { background: #888888ff; }")
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    assert_eq!(
        bg_of(&h.get_styles("#d")).as_deref(),
        Some("#888888ff"),
        "disabled state drives the rule"
    );
    h.assert_view_coherent();
}
