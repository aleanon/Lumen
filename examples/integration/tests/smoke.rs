use lumen_core::geometry::Size;

#[test]
fn hud_embeds_headlessly() {
    // The embedding contract: the host owns the loop; Lumen is pump +
    // screenshot + inject. All three work with zero shell involvement.
    let mut hud = integration::hud_app().run_headless(Size::new(260.0, 140.0));
    hud.pump();
    let before = hud.screenshot();
    assert_eq!((before.width(), before.height()), (260, 140));

    // Forward a "host" click into the HUD button.
    let t = hud.semantics_doc().root.elided();
    fn find(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<kurbo::Rect> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.bounds);
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    let b = find(&t, "bump").unwrap();
    let p = kurbo::Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    use lumen_core::events::{Event, PointerEvent};
    hud.inject(Event::PointerDown(PointerEvent::at(p)));
    hud.inject(Event::PointerUp(PointerEvent::at(p)));
    hud.pump();
    assert!(hud.semantics_json().to_string().contains("host clicks: 1"));
    assert_ne!(hud.screenshot().pixels(), before.pixels());
}
