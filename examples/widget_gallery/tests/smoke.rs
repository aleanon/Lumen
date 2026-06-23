//! Every widget in the gallery is wired up — drive each and assert the effect.

use lumen_core::events::{
    Event, Key, KeyEvent, Modifiers, NamedKey, PointerButton, PointerEvent, PointerKind,
    TextInputEvent,
};
use lumen_core::geometry::{Point, Rect, Size};
use lumen_core::semantics::SemanticsNode;
use lumen_widgets::Headless;

fn rect_id(n: &SemanticsNode, id: &str) -> Option<Rect> {
    if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
        return Some(n.bounds);
    }
    n.children.iter().find_map(|c| rect_id(c, id))
}
fn rect_label(n: &SemanticsNode, label: &str) -> Option<Rect> {
    if n.label == label {
        return Some(n.bounds);
    }
    n.children.iter().find_map(|c| rect_label(c, label))
}

fn app() -> Headless {
    widget_gallery::main_app().run_headless(Size::new(620.0, 980.0))
}
fn json(a: &Headless) -> String {
    a.semantics_json().to_string()
}
fn press(a: &mut Headless, p: Point) {
    let pe = PointerEvent {
        pos: p,
        button: PointerButton::Left,
        pointer: PointerKind::Mouse,
        modifiers: Default::default(),
        click_count: 1,
    };
    a.inject(Event::PointerDown(pe));
    a.inject(Event::PointerUp(pe));
    a.pump();
}
fn click(a: &mut Headless, id: &str) {
    let b = rect_id(&a.semantics_doc().root, id).unwrap_or_else(|| panic!("no #{id}"));
    press(a, Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0));
}
fn click_label(a: &mut Headless, label: &str) {
    let b = rect_label(&a.semantics_doc().root, label).unwrap_or_else(|| panic!("no '{label}'"));
    press(a, Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0));
}
fn key(a: &mut Headless, k: NamedKey) {
    a.inject(Event::KeyDown(KeyEvent {
        key: Key::Named(k),
        modifiers: Modifiers::empty(),
        repeat: false,
    }));
    a.pump();
}

#[test]
fn button_counts_and_resets() {
    let mut a = app();
    a.pump();
    assert!(json(&a).contains("Count: 0"));
    click(&mut a, "add-one");
    click(&mut a, "add-one");
    assert!(json(&a).contains("Count: 2"), "two presses");
    click(&mut a, "reset");
    assert!(json(&a).contains("Count: 0"), "reset");
}

#[test]
fn slider_drives_progress() {
    let mut a = app();
    a.pump();
    assert!(json(&a).contains("Volume: 35%"));
    // Press the slider at ~80% of its width → value ~80.
    let b = rect_id(&a.semantics_doc().root, "volume").unwrap();
    press(
        &mut a,
        Point::new(b.x0 + 0.8 * (b.x1 - b.x0), (b.y0 + b.y1) / 2.0),
    );
    assert!(
        json(&a).contains("Volume: 80%"),
        "slider set volume: {}",
        json(&a).contains("Volume")
    );
}

#[test]
fn checkbox_and_radio() {
    let mut a = app();
    a.pump();
    assert!(json(&a).contains("Notify: off"));
    click(&mut a, "notify");
    assert!(json(&a).contains("Notify: on"), "checkbox toggled");
    click(&mut a, "r-dark");
    assert!(json(&a).contains("Dark"));
    click(&mut a, "r-auto");
    assert!(json(&a).contains("Auto"));
}

#[test]
fn picklist_selects() {
    let mut a = app();
    a.pump();
    click(&mut a, "fruit"); // open
    assert!(json(&a).contains("Mango"), "options shown");
    click_label(&mut a, "Cherry");
    assert!(json(&a).contains("Cherry"), "selection shown");
    assert!(!json(&a).contains("Mango"), "menu closed");
}

#[test]
fn textinput_submit_adds_and_deletes() {
    let mut a = app();
    a.pump();
    assert!(json(&a).contains("0 item(s)"));

    // Focus the input, type, press Enter → a line is added and the field clears.
    click(&mut a, "draft");
    a.inject(Event::TextInput(TextInputEvent {
        text: "Buy milk".into(),
    }));
    a.pump();
    key(&mut a, NamedKey::Enter);
    let t = json(&a);
    assert!(t.contains("1 item(s)"), "submit added a line");
    assert!(t.contains("Buy milk"), "line text present");

    // The Add button also works.
    click(&mut a, "draft");
    a.inject(Event::TextInput(TextInputEvent {
        text: "Walk dog".into(),
    }));
    a.pump();
    click(&mut a, "add");
    assert!(json(&a).contains("2 item(s)"), "add button appended");

    // Each row's × deletes it.
    click_label(&mut a, "×");
    assert!(json(&a).contains("1 item(s)"), "row deleted");
}
