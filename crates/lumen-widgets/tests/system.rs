//! T5.2 acceptance: desktop system integration — drag-and-drop through the one
//! input path, clipboard, native-menu model + invoke, OS-service requests, and
//! multi-window descriptors — all headless-synthesizable.

use kurbo::{Point, Size};
use lumen_core::events::{DropData, DropEvent, Event};
use lumen_widgets::system::{MenuItem, MenuModel, SystemRequest, WindowDesc};
use lumen_widgets::{widgets, App, BuildCx, Element, Headless};

fn run(build: impl Fn(&mut BuildCx) -> Element + 'static) -> Headless {
    App::new(build).run_headless(Size::new(300.0, 200.0))
}

#[test]
fn drag_and_drop_delivers_payload() {
    let mut h = run(|cx| {
        let dropped = cx.signal("dropped", String::new);
        let v = dropped.get(cx.runtime());
        widgets::column(vec![widgets::text(format!("got: {v}")).id("label")])
            .id("zone")
            .on_drop(move |rt, data| dropped.set(rt, data.text.clone().unwrap_or_default()))
    });
    // Drop text onto the zone.
    h.inject(Event::Drop(DropEvent {
        pos: Point::new(20.0, 10.0),
        data: DropData {
            text: Some("payload.txt".into()),
            files: vec!["/a/payload.txt".into()],
        },
    }));
    h.pump();
    let root = h.semantics_doc().root.elided();
    fn find(n: &lumen_core::semantics::SemanticsNode, needle: &str) -> bool {
        n.label.contains(needle) || n.children.iter().any(|c| find(c, needle))
    }
    assert!(
        find(&root, "got: payload.txt"),
        "drop handler ran with payload"
    );
}

#[test]
fn clipboard_round_trips() {
    let mut h = run(|_| widgets::text("x"));
    assert_eq!(h.clipboard_read(), "");
    h.clipboard_write("copied!");
    assert_eq!(h.clipboard_read(), "copied!");
}

#[test]
fn menu_model_query_and_invoke() {
    let mut h = run(|_| widgets::text("x"));
    h.set_menu(MenuModel {
        items: vec![MenuItem::submenu(
            "file",
            "File",
            vec![
                MenuItem::new("file.open", "Open…"),
                MenuItem::new("file.save", "Save"),
            ],
        )],
    });
    assert!(h.menu().find("file.save").is_some());
    assert_eq!(h.invoke_menu("file.open").as_deref(), Some("Open…"));
    assert_eq!(h.invoke_menu("nope"), None);
    assert_eq!(h.invoked_menu(), ["file.open"]);
}

#[test]
fn system_requests_are_recorded() {
    let mut h = run(|_| widgets::text("x"));
    h.request_system(SystemRequest::Notification {
        title: "Done".into(),
        body: "Build finished".into(),
    });
    h.request_system(SystemRequest::OpenFile {
        filters: vec!["png".into()],
        reply: "pick.path".into(),
    });
    assert_eq!(h.system_requests().len(), 2);
    assert!(matches!(
        &h.system_requests()[0],
        SystemRequest::Notification { title, .. } if title == "Done"
    ));
}

#[test]
fn secondary_windows_are_listed() {
    let mut h = run(|_| widgets::text("x"));
    h.set_windows(vec![WindowDesc {
        id: "prefs".into(),
        title: "Preferences".into(),
        width: 400.0,
        height: 300.0,
    }]);
    assert_eq!(h.windows().len(), 1);
    assert_eq!(h.windows()[0].id, "prefs");
}
