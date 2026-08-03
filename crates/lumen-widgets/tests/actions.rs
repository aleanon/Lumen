//! W2 — a widget must implement every semantic action it declares.
//!
//! `actions` is the contract the agent (`input.invokeAction`) and AccessKit read
//! to decide what a node can do. Before this, `Slider`/`RangeSlider`/`Stepper`
//! declared `Increment`/`Decrement`/`SetValue` that nothing routed, and `Menu`
//! items declared `Click` with no handler at all — so the semantic tree
//! advertised capabilities that silently did nothing. That is precisely the
//! semantics-vs-reality drift ADR-009 exists to prevent.

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, widgets_m1, App, BuildCx, Element, Headless, Menu, Slider};

fn slider_app() -> Headless {
    App::new(|cx: &mut BuildCx| Slider::new(cx, "vol", 0.0, 100.0).id("vol").into())
        .run_headless(Size::new(300.0, 80.0))
}

fn vol(h: &Headless) -> f64 {
    let s: Signal<f64> = h.runtime().signal("vol", || 0.0);
    s.get(h.runtime())
}

#[test]
fn the_agent_can_increment_and_decrement_a_slider() {
    let mut h = slider_app();
    h.pump();
    assert_eq!(vol(&h), 0.0);

    h.invoke_action("#vol", "increment")
        .expect("a declared Increment must be routable");
    assert_eq!(vol(&h), 1.0, "one step of the default 1%-of-range");

    h.invoke_action("#vol", "increment").unwrap();
    h.invoke_action("#vol", "decrement").unwrap();
    assert_eq!(vol(&h), 1.0);
}

#[test]
fn the_agent_can_set_a_slider_value_directly() {
    let mut h = slider_app();
    h.pump();
    h.invoke_action_with("#vol", "setValue", Some("42.5"))
        .expect("a declared SetValue must be routable");
    assert_eq!(vol(&h), 42.5);

    // Out-of-range clamps rather than corrupting the value.
    h.invoke_action_with("#vol", "setValue", Some("1000"))
        .unwrap();
    assert_eq!(vol(&h), 100.0);
    // Unparseable input is ignored, not panicking.
    h.invoke_action_with("#vol", "setValue", Some("banana"))
        .unwrap();
    assert_eq!(vol(&h), 100.0);
}

/// The accessible value used to be `format!("{v:.0}")`, so a 0..1 slider read
/// `"0"` at every position — the agent and AT saw a control that never moved.
#[test]
fn a_fractional_slider_reports_a_meaningful_value() {
    let mut h = App::new(|cx: &mut BuildCx| Slider::new(cx, "f", 0.0, 1.0).id("f").into())
        .run_headless(Size::new(300.0, 80.0));
    h.pump();

    fn find(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<String> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return n.value.clone();
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    let read = |h: &Headless| find(&h.semantics_doc().root.elided(), "f").unwrap_or_default();
    let before = read(&h);
    h.invoke_action("#f", "increment").unwrap();
    let after = read(&h);
    assert_ne!(
        before, after,
        "a 0..1 slider must report a changing value, got {before:?} then {after:?}"
    );
}

#[test]
fn the_agent_can_drive_a_stepper() {
    let mut h =
        App::new(|cx: &mut BuildCx| widgets_m1::Stepper::new(cx, "qty", 0, 10).id("qty").into())
            .run_headless(Size::new(300.0, 80.0));
    h.pump();

    h.invoke_action("#qty", "increment").unwrap();
    h.invoke_action("#qty", "increment").unwrap();
    let n: Signal<i64> = h.runtime().signal("qty", || 0i64);
    assert_eq!(n.get(h.runtime()), 2);

    h.invoke_action("#qty", "decrement").unwrap();
    assert_eq!(n.get(h.runtime()), 1);

    h.invoke_action_with("#qty", "setValue", Some("7")).unwrap();
    assert_eq!(n.get(h.runtime()), 7);
}

/// `Menu` used to take no callback and wire no handlers, while its items
/// declared `Action::Click`. Choosing an item did nothing.
#[test]
fn menu_items_are_selectable() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let picked = cx.signal("picked", || -1i64);
        Menu::new(&["Cut", "Copy", "Paste"])
            .on_select(move |rt, i| picked.set(rt, i as i64))
            .id("m")
            .into()
    })
    .run_headless(Size::new(200.0, 160.0));
    h.pump();

    let picked: Signal<i64> = h.runtime().signal("picked", || -1i64);
    assert_eq!(picked.get(h.runtime()), -1);

    // Address the second item by its role+label through the agent path.
    h.invoke_action(r#"menu_item:text("Copy")"#, "click")
        .expect("a menu item must be actuable");
    assert_eq!(
        picked.get(h.runtime()),
        1,
        "the chosen index reaches the app"
    );
}

/// A `Menu` with no `on_select` must not *claim* Click — better to declare
/// nothing than to advertise a capability that does nothing.
#[test]
fn a_menu_without_a_handler_declares_no_click() {
    let mut h = App::new(|_cx: &mut BuildCx| Menu::new(&["A", "B"]).id("m").into())
        .run_headless(Size::new(200.0, 120.0));
    h.pump();
    assert!(
        h.audit_actions().is_empty(),
        "an inert menu must not advertise Click: {:?}",
        h.audit_actions()
    );
}

/// The regression net: assemble a screen of the widgets that declare actions and
/// assert none of them promises something it cannot do.
#[test]
fn no_widget_declares_an_action_it_does_not_implement() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let picked = cx.signal("picked", || 0i64);
        let items: Vec<Element> = vec![
            Slider::new(cx, "s", 0.0, 10.0).into(),
            widgets_m1::Stepper::new(cx, "n", 0, 5).into(),
            widgets_m1::Switch::new(cx, "sw", "Wi-Fi").into(),
            widgets_m1::Tabs::new(cx, "t", &["A", "B"]).into(),
            widgets::button("Go", |_| {}),
            widgets::text("static"),
            Menu::new(&["One", "Two"])
                .on_select(move |rt, i| picked.set(rt, i as i64))
                .into(),
            lumen_widgets::CheckBox::new(cx, "c", "Check").into(),
            lumen_widgets::Radio::new(cx, "g", "one", "One").into(),
            lumen_widgets::TextInput::new(cx, "ti", "hi").into(),
            lumen_widgets::Accordion::new(cx, "acc", "More").into(),
            lumen_widgets::PickList::new(cx, "pl", "Pick…", ["x", "y"]).into(),
        ];
        col(items)
    })
    .run_headless(Size::new(420.0, 900.0));
    h.pump();

    let diags = h.audit_actions();
    assert!(
        diags.is_empty(),
        "widgets promising unimplemented actions:\n{}",
        diags
            .iter()
            .map(|d| format!("  {} {}", d.code, d.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

fn col(items: Vec<Element>) -> Element {
    widgets::column(items)
}
