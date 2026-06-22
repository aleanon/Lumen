use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
use lumen_core::geometry::{Point, Size};
use lumen_widgets::Headless;

fn click(a: &mut Headless, id: &str) {
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
    let pe = PointerEvent {
        pos: Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0),
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
fn loads_profile_and_refetches() {
    // Default inline executor: a resource settles within two pumps.
    let mut a = data::main_app().run_headless(Size::new(460.0, 460.0));
    a.pump();
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(
        t.contains("Profile") && t.contains("Ada Lovelace"),
        "first profile loaded: {t}"
    );
    assert!(t.contains("ready"), "not loading after settle");

    // Load next → id bumps → refetch → second name appears.
    click(&mut a, "refresh");
    a.pump(); // drain the refetch result
    let t = a.semantics_json().to_string();
    assert!(
        t.contains("Alan Turing") && t.contains("#2"),
        "refetched next profile: {t}"
    );
}
