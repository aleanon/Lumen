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
fn seeds_conversation_and_echoes() {
    let mut a = websocket::main_app().run_headless(Size::new(520.0, 540.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("WebSocket") && t.contains("ONLINE"));
    assert!(t.contains("hello, socket"), "seeded conversation present");

    // Focus the composer, type, then Send: the message is echoed back.
    click(&mut a, "draft");
    a.inject(Event::TextInput(lumen_core::events::TextInputEvent {
        text: "pong".to_string(),
    }));
    a.pump();
    click(&mut a, "send");
    let t = a.semantics_json().to_string();
    assert!(t.matches("pong").count() >= 2, "sent message echoed");
}
