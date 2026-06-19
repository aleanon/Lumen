//! The hero number + pill recolour with the sign, via `.lss` classes toggled in
//! build — verified by both the status label and the painted colour.

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

fn has(a: &mut Headless, pred: fn(&[u8]) -> bool) -> bool {
    a.screenshot().pixels().chunks_exact(4).any(pred)
}

#[test]
fn sign_reactive_theme() {
    let mut a = counter::main_app().run_headless(Size::new(440.0, 520.0));
    a.pump();
    assert!(a.semantics_json().to_string().contains("AT ZERO"));

    click(&mut a, "inc10"); // -> 10
    let t = a.semantics_json().to_string();
    assert!(
        t.contains("POSITIVE") && t.contains("\"10\""),
        "positive at 10"
    );
    assert!(
        has(&mut a, |p| p[1] > 180 && p[0] < 120),
        "positive green painted"
    );

    click(&mut a, "dec10");
    click(&mut a, "dec10"); // 10 - 20 = -10
    let t = a.semantics_json().to_string();
    assert!(t.contains("NEGATIVE"), "negative");
    assert!(
        has(&mut a, |p| p[0] > 180 && p[1] < 150 && p[2] < 170),
        "rose painted"
    );

    click(&mut a, "reset");
    assert!(
        a.semantics_json().to_string().contains("AT ZERO"),
        "reset to zero"
    );
}
