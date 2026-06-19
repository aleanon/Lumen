//! Add / toggle / delete behaviour, plus the `.lss` done-green theming.

use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind, TextInputEvent};
use lumen_core::geometry::{Point, Size};
use lumen_widgets::Headless;

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

fn click(a: &mut Headless, id: &str) {
    let pe = PointerEvent {
        pos: center(a, id),
        button: PointerButton::Left,
        pointer: PointerKind::Mouse,
        modifiers: Default::default(),
        click_count: 1,
    };
    a.inject(Event::PointerDown(pe));
    a.inject(Event::PointerUp(pe));
    a.pump();
}

#[test]
fn add_toggle_delete() {
    let mut a = todos::main_app().run_headless(Size::new(520.0, 560.0));
    a.pump();
    assert!(
        a.semantics_json().to_string().contains("2 left"),
        "starts with 2 left"
    );

    // Toggle item 1 done -> 1 left, and the done green appears.
    click(&mut a, "check-1");
    assert!(a.semantics_json().to_string().contains("1 left"));
    assert!(
        a.screenshot()
            .pixels()
            .chunks_exact(4)
            .any(|p| p[1] > 140 && p[0] < 120 && p[2] < 120),
        "done check painted green"
    );

    // Delete item 0 -> 2 items remain.
    click(&mut a, "del-0");
    assert!(a.semantics_json().to_string().contains("of 2 done"));

    // Type into the field and add.
    click(&mut a, "draft"); // focus
    a.inject(Event::TextInput(TextInputEvent {
        text: "New task".to_string(),
    }));
    a.pump();
    click(&mut a, "add");
    let t = a.semantics_json().to_string();
    assert!(
        t.contains("New task") && t.contains("of 3 done"),
        "added a task"
    );
}
