//! The per-file typed widgets build correct elements, lower via `Into<Element>`,
//! render, and behave (a `Button` press mutates state).

use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
use lumen_core::geometry::{Point, Size};
use lumen_widgets::{
    App, BuildCx, Button, Container, Element, Headless, Label, Scrollable, Slider, TextInput,
};

fn center(a: &Headless, id: &str) -> Point {
    fn find(
        n: &lumen_core::semantics::SemanticsNode,
        id: &str,
    ) -> Option<lumen_core::geometry::Rect> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.bounds);
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    let b = find(&a.semantics_doc().root, id).unwrap();
    Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0)
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
    // Each widget lowers to an Element via From, and exposes its element() view.
    let b: Element = Button::new("x").ghost().into();
    assert_eq!(b.role, lumen_core::semantics::Role::Button);
    let l: Element = Label::new("hi").into();
    assert_eq!(l.role, lumen_core::semantics::Role::Text);
    let c = Container::new(vec![Label::new("a").into()])
        .row()
        .padding(4.0);
    assert_eq!(c.element().children.len(), 1);
}
