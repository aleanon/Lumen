use lumen_core::geometry::{Point, Rect, Size};
use lumen_widgets::Headless;

fn center(a: &Headless, id: &str) -> Point {
    fn find(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<Rect> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.bounds);
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    let b = find(&a.semantics_doc().root, id).unwrap();
    Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0)
}

fn click(a: &mut Headless, id: &str) {
    use lumen_core::events::*;
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
fn logs_events() {
    let mut a = events::main_app().run_headless(Size::new(460.0, 440.0));
    a.pump();
    assert!(a.semantics_json().to_string().contains("Event Inspector"));
    click(&mut a, "danger");
    assert!(
        a.semantics_json()
            .to_string()
            .contains("danger action fired"),
        "event logged"
    );
}
