use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
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
fn dialog_toggles_with_signal() {
    let mut a = modal::main_app().run_headless(Size::new(540.0, 460.0));
    a.pump();
    assert!(
        !a.semantics_json().to_string().contains("Delete file?"),
        "closed initially"
    );

    click(&mut a, "open");
    assert!(
        a.semantics_json().to_string().contains("Delete file?"),
        "dialog open"
    );

    click(&mut a, "cancel");
    assert!(
        !a.semantics_json().to_string().contains("Delete file?"),
        "dialog closed again"
    );
}
