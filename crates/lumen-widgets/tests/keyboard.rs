//! W3 — non-text widgets must be operable from the keyboard.
//!
//! App-level keying was already solid (Tab traversal, Enter/Space activation,
//! Escape dismiss), but `on_key` was implemented by only three files, so every
//! other widget ignored the keyboard beyond activation: a slider could not be
//! moved, a tab bar not traversed, a dropdown not chosen from, a scroll region
//! not scrolled. These follow the WAI-ARIA authoring patterns.

use kurbo::Size;
use lumen_core::events::{Event, Key, KeyEvent, Modifiers, NamedKey};
use lumen_core::state::Signal;
use lumen_widgets::{
    widgets, App, BuildCx, Element, Headless, PickList, RangeSlider, Scrollable, Slider,
};

fn key(h: &mut Headless, named: NamedKey) {
    press(h, named, Modifiers::empty())
}

fn press(h: &mut Headless, named: NamedKey, modifiers: Modifiers) {
    h.inject(Event::KeyDown(KeyEvent {
        key: Key::Named(named),
        modifiers,
        repeat: false,
    }));
    h.pump();
}

/// Tab to the (single) focusable widget, then drive it.
fn focus_first(h: &mut Headless) {
    key(h, NamedKey::Tab);
}

#[test]
fn arrow_keys_move_a_slider() {
    let mut h = App::new(|cx: &mut BuildCx| Slider::new(cx, "v", 0.0, 100.0).id("v").into())
        .run_headless(Size::new(300.0, 80.0));
    h.pump();
    focus_first(&mut h);

    let v: Signal<f64> = h.runtime().signal("v", || 0.0);
    key(&mut h, NamedKey::ArrowRight);
    assert_eq!(v.get(h.runtime()), 1.0, "→ steps up");
    key(&mut h, NamedKey::ArrowUp);
    assert_eq!(v.get(h.runtime()), 2.0, "↑ steps up too");
    key(&mut h, NamedKey::ArrowLeft);
    assert_eq!(v.get(h.runtime()), 1.0, "← steps down");

    key(&mut h, NamedKey::PageUp);
    assert_eq!(v.get(h.runtime()), 11.0, "PageUp moves ten steps");
    key(&mut h, NamedKey::Home);
    assert_eq!(v.get(h.runtime()), 0.0, "Home jumps to the minimum");
    key(&mut h, NamedKey::End);
    assert_eq!(v.get(h.runtime()), 100.0, "End jumps to the maximum");

    // Bounds hold.
    key(&mut h, NamedKey::ArrowRight);
    assert_eq!(v.get(h.runtime()), 100.0, "cannot exceed the maximum");
}

#[test]
fn a_custom_step_is_honoured_by_the_keyboard() {
    let mut h =
        App::new(|cx: &mut BuildCx| Slider::new(cx, "v", 0.0, 10.0).step(0.5).id("v").into())
            .run_headless(Size::new(300.0, 80.0));
    h.pump();
    focus_first(&mut h);

    let v: Signal<f64> = h.runtime().signal("v", || 0.0);
    key(&mut h, NamedKey::ArrowRight);
    assert_eq!(v.get(h.runtime()), 0.5, "one custom step");
}

#[test]
fn arrow_keys_move_a_range_sliders_ends() {
    let mut h = App::new(|cx: &mut BuildCx| RangeSlider::new(cx, "r", 0.0, 100.0).id("r").into())
        .run_headless(Size::new(300.0, 80.0));
    h.pump();
    focus_first(&mut h);

    let lo: Signal<f64> = h.runtime().signal("r.lo", || 0.0);
    let hi: Signal<f64> = h.runtime().signal("r.hi", || 100.0);

    key(&mut h, NamedKey::ArrowLeft);
    assert_eq!(hi.get(h.runtime()), 99.0, "plain arrows move the upper end");
    assert_eq!(lo.get(h.runtime()), 0.0, "and leave the lower end alone");

    press(&mut h, NamedKey::ArrowRight, Modifiers::SHIFT);
    assert_eq!(lo.get(h.runtime()), 1.0, "shift+arrow moves the lower end");
}

#[test]
fn arrow_keys_traverse_a_tab_bar() {
    let mut h = App::new(|cx: &mut BuildCx| {
        widgets::Tabs::new(cx, "t", &["One", "Two", "Three"])
            .id("t")
            .into()
    })
    .run_headless(Size::new(300.0, 60.0));
    h.pump();
    focus_first(&mut h);

    let t: Signal<usize> = h.runtime().signal("t", || 0usize);
    key(&mut h, NamedKey::ArrowRight);
    assert_eq!(t.get(h.runtime()), 1, "→ selects the next tab");
    key(&mut h, NamedKey::End);
    assert_eq!(t.get(h.runtime()), 2, "End selects the last tab");
    key(&mut h, NamedKey::ArrowRight);
    assert_eq!(t.get(h.runtime()), 0, "→ wraps around, as a tablist does");
    key(&mut h, NamedKey::Home);
    assert_eq!(t.get(h.runtime()), 0, "Home selects the first");
}

#[test]
fn arrow_keys_choose_from_a_dropdown() {
    let mut h = App::new(|cx: &mut BuildCx| {
        PickList::new(cx, "p", "Pick…", ["Red", "Green", "Blue"])
            .id("p")
            .into()
    })
    .run_headless(Size::new(260.0, 220.0));
    h.pump();
    focus_first(&mut h);

    let sel: Signal<String> = h.runtime().signal("p", String::new);
    assert_eq!(sel.get(h.runtime()), "", "nothing chosen yet");

    key(&mut h, NamedKey::ArrowDown);
    assert_eq!(sel.get(h.runtime()), "Red", "↓ opens and takes the first");
    key(&mut h, NamedKey::ArrowDown);
    assert_eq!(sel.get(h.runtime()), "Green");
    key(&mut h, NamedKey::ArrowUp);
    assert_eq!(sel.get(h.runtime()), "Red");
    key(&mut h, NamedKey::End);
    assert_eq!(sel.get(h.runtime()), "Blue", "End takes the last option");

    // Escape closes the list without changing the choice.
    let open: Signal<bool> = h.runtime().signal("p.open", || false);
    assert!(open.get(h.runtime()), "arrowing opened the list");
    key(&mut h, NamedKey::Escape);
    assert!(!open.get(h.runtime()), "Escape closes it");
    assert_eq!(sel.get(h.runtime()), "Blue", "and keeps the selection");
}

#[test]
fn a_scroll_region_is_keyboard_operable() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let rows: Vec<Element> = (0..40)
            .map(|i| lumen_widgets::widgets::text(format!("row {i}")))
            .collect();
        Scrollable::new(cx, "sc", 100.0, 800.0, rows)
            .id("sc")
            .into()
    })
    .run_headless(Size::new(220.0, 140.0));
    h.pump();
    focus_first(&mut h);

    let off: Signal<f64> = h.runtime().signal("sc", || 0.0);
    assert_eq!(off.get(h.runtime()), 0.0);

    key(&mut h, NamedKey::ArrowDown);
    assert!(off.get(h.runtime()) > 0.0, "↓ scrolls a line");

    key(&mut h, NamedKey::End);
    assert_eq!(off.get(h.runtime()), 700.0, "End goes to the bottom");
    key(&mut h, NamedKey::Home);
    assert_eq!(off.get(h.runtime()), 0.0, "Home goes back to the top");

    key(&mut h, NamedKey::PageDown);
    let after_page = off.get(h.runtime());
    assert!(after_page > 0.0, "PageDown scrolls");
    key(&mut h, NamedKey::PageUp);
    assert_eq!(off.get(h.runtime()), 0.0, "PageUp returns");
    h.assert_view_coherent();
}
