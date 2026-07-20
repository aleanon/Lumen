use lumen_core::geometry::Size;
use lumen_widgets::system::SystemRequest;

#[test]
fn confirm_flow_queues_the_exit_request() {
    let mut a = exit::main_app().run_headless(Size::new(420.0, 320.0));
    a.pump();
    // Arm, then confirm — the portable request is recorded for the shell.
    for id in ["#arm", "#exit"] {
        let t = a.semantics_doc().root.elided();
        fn find(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<kurbo::Rect> {
            if n.id.as_ref().map(|i| i.as_str()) == Some(&id[1..]) {
                return Some(n.bounds);
            }
            n.children.iter().find_map(|c| find(c, id))
        }
        let b = find(&t, id).unwrap_or_else(|| panic!("{id} present"));
        let p = kurbo::Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
        use lumen_core::events::{Event, PointerEvent};
        a.inject(Event::PointerDown(PointerEvent::at(p)));
        a.inject(Event::PointerUp(PointerEvent::at(p)));
        a.pump();
    }
    assert!(
        a.system_requests().contains(&SystemRequest::Exit),
        "exit request recorded: {:?}",
        a.system_requests()
    );
}
