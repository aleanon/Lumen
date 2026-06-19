//! The typed-builder facade: `Button` exposes only relevant modifiers and lowers
//! to a normal Element; `col!` mixes typed widgets and raw Elements.

use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
use lumen_core::geometry::{Point, Size};
use lumen_widgets::{col, widgets, App, BuildCx, Button, Headless};

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
