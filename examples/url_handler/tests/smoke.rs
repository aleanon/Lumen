use lumen_core::geometry::Size;

#[test]
fn deep_link_routes_and_carries_params() {
    let mut a = url_handler::main_app().run_headless(Size::new(520.0, 320.0));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("Home"), "starts at home");

    fn click(a: &mut lumen_widgets::Headless, id: &str) {
        let t = a.semantics_doc().root.elided();
        fn find(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<kurbo::Rect> {
            if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
                return Some(n.bounds);
            }
            n.children.iter().find_map(|c| find(c, id))
        }
        let b = find(&t, id).unwrap();
        let p = kurbo::Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
        use lumen_core::events::{Event, PointerEvent};
        a.inject(Event::PointerDown(PointerEvent::at(p)));
        a.inject(Event::PointerUp(PointerEvent::at(p)));
        a.pump();
    }
    click(&mut a, "link-profile");
    let t = a.semantics_json().to_string();
    assert!(
        t.contains("Profile: ada"),
        "deep link carried the parameter: {t}"
    );
}
