//! The typed-builder facade: `Button` exposes only relevant modifiers and lowers
//! to a normal Element; `col!` mixes typed widgets and raw Elements.

use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
use lumen_core::geometry::{Point, Size};
use lumen_widgets::{
    col, widgets, App, BuildCx, Button, Checkbox, Element, Headless, Slider, Text, TextField,
};

fn center(a: &Headless, id: &str) -> Point {
    fn find(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<kurbo::Rect> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.bounds);
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    let b = find(&a.semantics_doc().root, id).unwrap();
    Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0)
}

#[test]
fn typed_button_builds_lowers_and_clicks() {
    let mut a = App::new(|cx: &mut BuildCx| {
        let n = cx.signal("n", || 0i32);
        let v = n.get(cx.runtime());
        // Heterogeneous children: a typed Button and a raw text Element.
        col![
            Button::new("Inc")
                .primary()
                .on_press(move |rt| n.update(rt, |x| *x += 1))
                .id("inc"),
            widgets::text(format!("count: {v}")).id("label"),
        ]
    })
    .run_headless(Size::new(160.0, 120.0));
    a.pump();

    // The typed Button lowered to a real Button node with its id + role.
    let tree = a.semantics_json().to_string();
    assert!(tree.contains("\"inc\""), "typed button id present");
    assert!(tree.contains("count: 0"), "raw text child present");

    // Its press handler runs through the normal input path.
    let c = center(&a, "inc");
    let pe = PointerEvent {
        pos: c,
        button: PointerButton::Left,
        pointer: PointerKind::Mouse,
        modifiers: Default::default(),
        click_count: 1,
    };
    a.inject(Event::PointerDown(pe));
    a.inject(Event::PointerUp(pe));
    a.pump();
    assert!(
        a.semantics_json().to_string().contains("count: 1"),
        "typed button's on_press fired"
    );

    // Type safety (compile-time, not asserted here): `Button` exposes no
    // `.on_drag()` / `.letter_spacing()` — those methods simply don't exist on it.
}

#[test]
fn typed_text_typography_lowers() {
    // Text typography flows through to a measurable layout difference.
    let plain: Element = Text::new("WWWWW").into();
    let spaced: Element = Text::new("WWWWW").letter_spacing(6.0).bold().into();
    let mut a = App::new(move |_cx: &mut BuildCx| col![plain.clone(), spaced.clone()])
        .run_headless(Size::new(300.0, 120.0));
    a.pump();
    // (Just exercising the build path; the text-stack tests already prove the
    // letter-spacing widens. Here we assert the typed widgets build + render.)
    assert!(a.semantics_json().to_string().contains("WWWWW"));
}

#[test]
fn typed_stateful_widgets_build() {
    // The stateful typed widgets lower and appear in the tree, mixed in a col!.
    let mut a = App::new(|cx: &mut BuildCx| {
        col![
            Text::new("Settings").bold().size(20.0).id("title"),
            Checkbox::new(cx, "notify", "Notify me").id("chk"),
            Slider::new(cx, "vol", 0.0, 100.0).id("vol"),
            TextField::new(cx, "name", "Ada").id("name"),
        ]
    })
    .run_headless(Size::new(280.0, 220.0));
    a.pump();
    let tree = a.semantics_json().to_string();
    for id in [
        "\"title\"",
        "\"chk\"",
        "\"vol\"",
        "\"name\"",
        "Notify me",
        "Ada",
    ] {
        assert!(tree.contains(id), "missing {id} in {tree}");
    }
}
