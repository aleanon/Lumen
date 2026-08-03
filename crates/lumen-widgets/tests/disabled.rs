//! W1 — a disabled widget must be disabled *everywhere*, not just in paint.
//!
//! Before this landed, `NodeFlags::DISABLED` was declared and read by
//! `build_semantics` but **never set by anything**, and hit-testing never
//! consulted it. So the only way to render a disabled-looking control was to
//! push `SemState::Disabled` into `states` by hand — producing a node that told
//! the agent and assistive tech "I am disabled" while still happily handling
//! clicks. These tests pin every path shut: pointer, keyboard, focus traversal,
//! the agent's geometry-free actuation, and inheritance to the subtree.

use kurbo::{Point, Size};
use lumen_core::events::{Event, Key, KeyEvent, Modifiers, NamedKey, PointerEvent};
use lumen_core::semantics::{SemanticsNode, State as SemState};
use lumen_core::state::Signal;
use lumen_widgets::{col, widgets, App, BuildCx, Button, Element, Headless};

fn click_at(h: &mut Headless, p: Point) {
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
}

fn key(h: &mut Headless, named: NamedKey) {
    h.inject(Event::KeyDown(KeyEvent {
        key: Key::Named(named),
        modifiers: Modifiers::empty(),
        repeat: false,
    }));
    h.pump();
}

fn sem(h: &Headless) -> SemanticsNode {
    h.semantics_doc().root.elided()
}

/// Which id currently reports `Focused`, read from the semantic tree — the
/// observable contract (there is no public focus accessor, by design).
fn focused(n: &SemanticsNode) -> Option<String> {
    if n.states.contains(&SemState::Focused) {
        return n.id.as_ref().map(|i| i.as_str().to_string());
    }
    n.children.iter().find_map(focused)
}

fn by_id<'a>(n: &'a SemanticsNode, id: &str) -> Option<&'a SemanticsNode> {
    if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
        return Some(n);
    }
    n.children.iter().find_map(|c| by_id(c, id))
}

/// One enabled and one disabled button, both wired to the same counter.
fn two_buttons() -> Headless {
    App::new(|cx: &mut BuildCx| {
        let hits = cx.signal("hits", || 0i64);
        let on: Element = Button::new("Enabled")
            .on_press(move |rt| hits.update(rt, |n| *n += 1))
            .id("on")
            .into();
        let off: Element = Button::new("Disabled")
            .on_press(move |rt| hits.update(rt, |n| *n += 1))
            .disabled(true)
            .id("off")
            .into();
        col![on, off]
    })
    .run_headless(Size::new(240.0, 160.0))
}

fn hits(h: &Headless) -> i64 {
    let s: Signal<i64> = h.runtime().signal("hits", || 0i64);
    s.get(h.runtime())
}

#[test]
fn a_disabled_button_ignores_clicks() {
    let mut h = two_buttons();
    h.pump();

    let on = h.node_bounds_by_id("on").expect("enabled laid out");
    let off = h.node_bounds_by_id("off").expect("disabled laid out");

    // Sanity: the enabled one works, so a miss below means "refused", not
    // "mis-aimed".
    click_at(
        &mut h,
        Point::new((on.x0 + on.x1) / 2.0, (on.y0 + on.y1) / 2.0),
    );
    assert_eq!(hits(&h), 1, "the enabled button fires");

    click_at(
        &mut h,
        Point::new((off.x0 + off.x1) / 2.0, (off.y0 + off.y1) / 2.0),
    );
    assert_eq!(hits(&h), 1, "a disabled button must not fire on click");
}

#[test]
fn a_disabled_button_reports_the_state_to_the_agent() {
    let mut h = two_buttons();
    h.pump();
    let s = sem(&h);
    assert!(
        by_id(&s, "off")
            .expect("disabled node in semantics")
            .states
            .contains(&SemState::Disabled),
        "the disabled control must say so in the semantic tree"
    );
    assert!(
        !by_id(&s, "on")
            .unwrap()
            .states
            .contains(&SemState::Disabled),
        "the enabled control must not"
    );
}

/// The important one: the state must not be a *claim*. Whatever the tree says,
/// the agent's geometry-free actuation has to agree with the pointer.
#[test]
fn the_agent_cannot_actuate_a_disabled_control() {
    let mut h = two_buttons();
    h.pump();

    assert!(
        h.invoke_action("#on", "click").is_ok(),
        "the enabled control is actuable"
    );
    assert_eq!(hits(&h), 1);

    let err = h
        .invoke_action("#off", "click")
        .expect_err("a disabled control must refuse invokeAction");
    assert!(
        err.contains("disabled"),
        "the refusal should say why, got {err:?}"
    );
    assert_eq!(hits(&h), 1, "and must not have run the handler");
}

#[test]
fn tab_traversal_skips_disabled_controls() {
    let mut h = two_buttons();
    h.pump();

    // Two focusables exist, but only one is reachable, so Tab cycles on it.
    key(&mut h, NamedKey::Tab);
    assert_eq!(
        focused(&sem(&h)).as_deref(),
        Some("on"),
        "first Tab lands on the enabled control"
    );
    key(&mut h, NamedKey::Tab);
    assert_eq!(
        focused(&sem(&h)).as_deref(),
        Some("on"),
        "Tab must wrap past the disabled control, never onto it"
    );
}

#[test]
fn keyboard_activation_cannot_reach_a_disabled_control() {
    let mut h = two_buttons();
    h.pump();
    key(&mut h, NamedKey::Tab); // focus the enabled one
    key(&mut h, NamedKey::Enter);
    assert_eq!(hits(&h), 1, "Enter activates the focused enabled control");

    // Focus can never move to the disabled one, so Enter can never fire it.
    key(&mut h, NamedKey::Tab);
    key(&mut h, NamedKey::Enter);
    assert_eq!(
        hits(&h),
        2,
        "still the enabled control, never the disabled one"
    );
}

/// Disabling a container disables everything inside it — the HTML/Flutter rule.
#[test]
fn disabling_a_container_disables_the_whole_subtree() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let hits = cx.signal("hits", || 0i64);
        let inner: Element =
            widgets::button("Inner", move |rt| hits.update(rt, |n| *n += 1)).id("inner");
        let mut group: Element = col![inner];
        group.disabled = true;
        group.id("group")
    })
    .run_headless(Size::new(240.0, 160.0));
    h.pump();

    let b = h.node_bounds_by_id("inner").expect("inner laid out");
    click_at(&mut h, Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0));
    assert_eq!(hits(&h), 0, "a child of a disabled container must not fire");

    assert!(
        by_id(&sem(&h), "inner")
            .unwrap()
            .states
            .contains(&SemState::Disabled),
        "the child inherits the disabled state in semantics too"
    );
    assert!(
        h.invoke_action("#inner", "click").is_err(),
        "and the agent cannot actuate it either"
    );
}

/// A control that is disabled only some of the time must come back to life.
#[test]
fn re_enabling_restores_input() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let locked = cx.signal("locked", || true);
        let hits = cx.signal("hits", || 0i64);
        Button::new("Go")
            .on_press(move |rt| hits.update(rt, |n| *n += 1))
            .disabled(locked.get(cx.runtime()))
            .id("go")
            .into()
    })
    .run_headless(Size::new(240.0, 120.0));
    h.pump();

    let b = h.node_bounds_by_id("go").expect("laid out");
    let c = Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    click_at(&mut h, c);
    assert_eq!(hits(&h), 0, "disabled while locked");

    let locked: Signal<bool> = h.runtime().signal("locked", || true);
    locked.set(h.runtime(), false);
    h.pump();

    click_at(&mut h, c);
    assert_eq!(hits(&h), 1, "re-enabled and clickable again");
    assert!(!by_id(&sem(&h), "go")
        .unwrap()
        .states
        .contains(&SemState::Disabled));
    h.assert_view_coherent();
}

/// `styling-lss` documents `button:disabled { … }` as a working selector. Before
/// W1 it could never match, because nothing emitted the state. Now it can — this
/// closes the documented-but-unreachable gap.
#[test]
fn the_lss_disabled_selector_matches() {
    let mut h = App::new(|_cx: &mut BuildCx| {
        let a: Element = Button::new("A").id("a").into();
        let b: Element = Button::new("B").disabled(true).id("b").into();
        col![a, b]
    })
    .stylesheet("button:disabled { background: #888888ff; }")
    .run_headless(Size::new(240.0, 160.0));
    h.pump();

    let off = h.get_styles("#b");
    assert_eq!(
        off["background"]["value"], "#888888ff",
        "the :disabled rule must apply, got {off:?}"
    );
    let on = h.get_styles("#a");
    assert_ne!(
        on["background"]["value"], "#888888ff",
        "and must not leak onto the enabled one"
    );
}
