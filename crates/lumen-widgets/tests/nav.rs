//! T5.4 acceptance: router (navigate/back/deep-link/guards), undo/redo history,
//! agent-driven navigation, and persistence of nav + undo state through a
//! tier-3 snapshot/restore.

use kurbo::Size;
use lumen_widgets::nav::Router;
use lumen_widgets::undo::History;
use lumen_widgets::{widgets, App, BuildCx, Element, Headless};

#[test]
fn router_navigate_back_deeplink_guard() {
    let mut r = Router::new("home");
    assert_eq!(r.current(), "home");
    r.navigate("settings");
    r.navigate("about");
    assert_eq!(r.current(), "about");
    assert_eq!(r.depth(), 3);
    assert!(r.back());
    assert_eq!(r.current(), "settings");
    assert!(r.back());
    assert!(!r.back(), "can't go back past the root");

    r.deep_link("settings/appearance/theme");
    assert_eq!(r.current(), "theme");
    assert_eq!(r.depth(), 3);

    // A guard blocks navigation to a forbidden route.
    assert!(!r.navigate_guarded("admin", |route| route != "admin"));
    assert_eq!(r.current(), "theme");
}

#[test]
fn undo_redo_history() {
    let mut h = History::new(0i32);
    h.push(1);
    h.push(2);
    assert_eq!(*h.present(), 2);
    assert!(h.can_undo() && !h.can_redo());
    assert!(h.undo());
    assert_eq!(*h.present(), 1);
    assert!(h.redo());
    assert_eq!(*h.present(), 2);
    // A new edit clears the redo stack.
    h.undo();
    h.push(9);
    assert!(!h.can_redo());
    assert_eq!(*h.present(), 9);
}

// An app whose route + counter (with undo) live in signals.
fn app(cx: &mut BuildCx) -> Element {
    let router = cx.signal("router", || Router::new("home"));
    let history = cx.signal("hist", || History::new(0i32));
    let route = router.get(cx.runtime()).current().to_string();
    let count = *history.get(cx.runtime()).present();

    let to_settings = widgets::button("Settings", move |rt| {
        router.update(rt, |r| r.navigate("settings"))
    })
    .id("to-settings");
    let back = widgets::button("Back", move |rt| {
        router.update(rt, |r| {
            r.back();
        })
    })
    .id("back");
    let inc = widgets::button("+1", move |rt| {
        history.update(rt, |h| {
            let n = *h.present() + 1;
            h.push(n)
        })
    })
    .id("inc");
    let undo = widgets::button("Undo", move |rt| {
        history.update(rt, |h| {
            h.undo();
        })
    })
    .id("undo");

    widgets::column(vec![
        widgets::text(format!("route: {route}")).id("route"),
        widgets::text(format!("count: {count}")).id("count"),
        to_settings,
        back,
        inc,
        undo,
    ])
}

fn label(h: &Headless, id: &str) -> String {
    fn find(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<String> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.label.clone());
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    find(&h.semantics_doc().root.elided(), id).unwrap()
}
fn click(h: &mut Headless, id: &str) {
    use lumen_core::events::{Event, PointerEvent};
    fn bounds(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<kurbo::Rect> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.bounds);
        }
        n.children.iter().find_map(|c| bounds(c, id))
    }
    let b = bounds(&h.semantics_doc().root.elided(), id).unwrap();
    let p = kurbo::Point::new(b.x0 + b.width() / 2.0, b.y0 + b.height() / 2.0);
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
}

#[test]
fn nav_and_undo_persist_through_restart() {
    let size = Size::new(300.0, 260.0);
    let mut h = App::new(app).run_headless(size);

    // Navigate + edit + undo.
    click(&mut h, "to-settings");
    assert_eq!(label(&h, "route"), "route: settings");
    click(&mut h, "inc");
    click(&mut h, "inc");
    assert_eq!(label(&h, "count"), "count: 2");
    click(&mut h, "undo");
    assert_eq!(label(&h, "count"), "count: 1");

    // Snapshot → drop → restore: nav stack + undo history survive (tier 3).
    let snap = h.snapshot();
    let wire = serde_json::to_string(&snap).unwrap();
    drop(h);
    let restored = serde_json::from_str(&wire).unwrap();
    let (h2, diags) = App::new(app).run_headless_restored(size, restored);
    assert!(diags.is_empty());
    assert_eq!(label(&h2, "route"), "route: settings", "route persisted");
    assert_eq!(label(&h2, "count"), "count: 1", "undo state persisted");
}
