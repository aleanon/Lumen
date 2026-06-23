//! The per-file typed widgets build correct elements, lower via `Into<Element>`,
//! render, and behave (a `Button` press mutates state).

use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
use lumen_core::geometry::{Point, Rect, Size};
use lumen_core::semantics::SemanticsNode;
use lumen_widgets::{
    App, BuildCx, Button, CheckBox, Container, Element, Headless, Label, PickList, ProgressBar,
    Radio, Rule, Scrollable, Slider, Space, TextField, TextInput,
};

fn find_id(n: &SemanticsNode, id: &str) -> Option<Rect> {
    if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
        return Some(n.bounds);
    }
    n.children.iter().find_map(|c| find_id(c, id))
}

fn find_label(n: &SemanticsNode, label: &str) -> Option<Rect> {
    if n.label == label {
        return Some(n.bounds);
    }
    n.children.iter().find_map(|c| find_label(c, label))
}

fn center(a: &Headless, id: &str) -> Point {
    let b = find_id(&a.semantics_doc().root, id).unwrap();
    Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0)
}

fn click_at(a: &mut Headless, p: Point) {
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
    let p = center(a, id);
    click_at(a, p);
}

fn click_label(a: &mut Headless, label: &str) {
    let b =
        find_label(&a.semantics_doc().root, label).unwrap_or_else(|| panic!("no label {label}"));
    click_at(a, Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0));
}

fn ui(cx: &mut BuildCx) -> Element {
    let saved = cx.signal("saved", || false);
    Container::new(vec![
        Label::new("Catalog").bold().size(20.0).id("title").into(),
        Label::new(if saved.get(cx.runtime()) {
            "saved!"
        } else {
            "unsaved"
        })
        .id("status")
        .into(),
        TextInput::new(cx, "name", "Ada Lovelace").id("name").into(),
        Slider::new(cx, "volume", 0.0, 100.0).id("volume").into(),
        Button::new("Save")
            .primary()
            .id("save")
            .on_press(move |rt| saved.set(rt, true))
            .into(),
        Scrollable::new(cx, "list", 80.0, 400.0, vec![Label::new("row 1").into()])
            .id("list")
            .into(),
    ])
    .padding(16.0)
    .gap(8.0)
    .id("root")
    .into()
}

#[test]
fn widgets_build_render_and_interact() {
    let mut a = App::new(ui).run_headless(Size::new(320.0, 520.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("Catalog"), "Label renders");
    assert!(t.contains("Ada Lovelace"), "TextInput value renders");
    assert!(t.contains("Save"), "Button label renders");
    assert!(t.contains("unsaved"), "initial state");

    // Press the button → its handler flips state.
    let p = center(&a, "save");
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
    assert!(
        a.semantics_json().to_string().contains("saved!"),
        "Button press mutated state"
    );
}

#[test]
fn into_element_lowers_each_widget() {
    use lumen_core::semantics::Role;
    let b: Element = Button::new("x").ghost().into();
    assert_eq!(b.role, Role::Button);
    let l: Element = Label::new("hi").into();
    assert_eq!(l.role, Role::Text);
    let c = Container::new(vec![Label::new("a").into()])
        .row()
        .padding(4.0);
    assert_eq!(c.element().children.len(), 1);
    assert_eq!(Element::from(ProgressBar::new(0.5)).role, Role::Progress);
    assert_eq!(Element::from(Rule::horizontal()).role, Role::Generic);
    assert_eq!(Element::from(Space::new()).role, Role::Generic);
}

/// A UI exercising the stateful widgets, mirroring their signals into labels so
/// the test can observe state via semantics.
fn form(cx: &mut BuildCx) -> Element {
    let agree = cx.signal("agree", || false);
    let color = cx.signal("color", String::new);
    let fruit = cx.signal("fruit", String::new);
    Container::new(vec![
        CheckBox::new(cx, "agree", "I agree").id("agree-box").into(),
        Label::new(format!("agree={}", agree.get(cx.runtime()))).into(),
        Radio::new(cx, "color", "red", "Red").id("r-red").into(),
        Radio::new(cx, "color", "green", "Green")
            .id("r-green")
            .into(),
        Label::new(format!("color={}", color.get(cx.runtime()))).into(),
        PickList::new(cx, "fruit", "Pick a fruit", ["Apple", "Banana", "Cherry"])
            .id("picker")
            .into(),
        Label::new(format!("fruit={}", fruit.get(cx.runtime()))).into(),
        ProgressBar::new(0.6).id("bar").into(),
        Rule::horizontal().into(),
        Space::vertical(8.0).into(),
        TextField::new(cx, "notes", "hello").id("notes").into(),
    ])
    .padding(12.0)
    .gap(6.0)
    .into()
}

#[test]
fn checkbox_radio_picklist_progress() {
    let mut a = App::new(form).run_headless(Size::new(360.0, 760.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("agree=false"), "checkbox starts unchecked");
    assert!(t.contains("Pick a fruit"), "picklist placeholder");
    assert!(t.contains("hello"), "textfield value renders");

    // CheckBox toggles.
    click(&mut a, "agree-box");
    assert!(
        a.semantics_json().to_string().contains("agree=true"),
        "checkbox toggled"
    );

    // Radio group is mutually exclusive.
    click(&mut a, "r-green");
    assert!(a.semantics_json().to_string().contains("color=green"));
    click(&mut a, "r-red");
    assert!(
        a.semantics_json().to_string().contains("color=red"),
        "radio switched"
    );

    // PickList: open, then choose an option.
    click(&mut a, "picker");
    assert!(
        a.semantics_json().to_string().contains("Banana"),
        "options shown when open"
    );
    click_label(&mut a, "Banana");
    let t = a.semantics_json().to_string();
    assert!(t.contains("fruit=Banana"), "selection stored");
    assert!(!t.contains("Apple"), "menu closed after choosing");
}

#[test]
fn picklist_dismisses_on_click_away_and_escape() {
    use lumen_core::events::{Key, KeyEvent, Modifiers, NamedKey};

    // Click-away: open the menu, then press outside it → it closes.
    let mut a = App::new(form).run_headless(Size::new(360.0, 760.0));
    a.pump();
    click(&mut a, "picker");
    assert!(
        a.semantics_json().to_string().contains("Cherry"),
        "menu open"
    );
    click_at(&mut a, Point::new(4.0, 4.0)); // far from the dropdown
    assert!(
        !a.semantics_json().to_string().contains("Cherry"),
        "outside press dismissed the menu"
    );

    // Escape: open again, press Escape → it closes.
    click(&mut a, "picker");
    assert!(
        a.semantics_json().to_string().contains("Cherry"),
        "menu re-open"
    );
    a.inject(Event::KeyDown(KeyEvent {
        key: Key::Named(NamedKey::Escape),
        modifiers: Modifiers::empty(),
        repeat: false,
    }));
    a.pump();
    assert!(
        !a.semantics_json().to_string().contains("Cherry"),
        "Escape dismissed the menu"
    );
}
