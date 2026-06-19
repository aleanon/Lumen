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
fn fractal_and_depth() {
    let mut a = sierpinski::main_app().run_headless(Size::new(460.0, 560.0));
    a.pump();
    assert!(a.semantics_json().to_string().contains("depth 5"));
    assert!(
        a.screenshot()
            .pixels()
            .chunks_exact(4)
            .any(|p| p[2] > 180 && p[0] > 80 && p[0] < 170),
        "fractal drawn"
    );
    click(&mut a, "inc");
    assert!(a.semantics_json().to_string().contains("depth 6"));
}
