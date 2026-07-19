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
fn activate_menu_runs_the_command_registered_under_the_same_id() {
    // P.3c: a native activation (muda click / accelerator / agent
    // `menu.invoke`) both records the invocation and runs the bound command.
    let mut h = run(|cx| {
        let n = cx.signal("count", || 0i32);
        cx.register_command("edit.bump", move |rt| n.update(rt, |v| *v += 1));
        widgets::text("x")
    });
    h.set_menu(MenuModel {
        items: vec![
            MenuItem::new("edit.bump", "Bump").accel("Ctrl+B"),
            MenuItem::new("help.about", "About"), // no bound command
        ],
    });
    assert_eq!(h.activate_menu("edit.bump").as_deref(), Some("Bump"));
    let n: lumen_core::state::Signal<i32> = h.runtime().signal("count", || 0);
    assert_eq!(n.get(h.runtime()), 1, "bound command ran");
    // Unbound items still record + pump without error.
    assert_eq!(h.activate_menu("help.about").as_deref(), Some("About"));
    assert_eq!(h.invoked_menu(), ["edit.bump", "help.about"]);
    // menu_rev tracks installs (the shell's rebuild trigger).
    assert_eq!(h.menu_rev(), 1);
    h.set_menu(MenuModel::default());
    assert_eq!(h.menu_rev(), 2);
}

#[test]
fn build_declared_menu_installs_once_per_change() {
    // P.3c: `cx.set_menu` declares the menu from build; identical models
    // must not churn menu_rev (the shell's native-menu rebuild trigger).
    let mut h = run(|cx| {
        let label = cx.signal("label", || "Open".to_string());
        cx.set_menu(MenuModel {
            items: vec![MenuItem::new("file.open", label.get(cx.runtime())).accel("Ctrl+O")],
        });
        widgets::text("x")
    });
    assert_eq!(h.menu().find("file.open").unwrap().label, "Open");
    assert_eq!(h.menu_rev(), 1);
    h.pump();
    h.pump();
    assert_eq!(h.menu_rev(), 1, "unchanged model must not reinstall");
    // A state change flowing into the model reinstalls it.
    let label = h.runtime().signal("label", || "Open".to_string());
    label.set(h.runtime(), "Öppna".into());
    h.pump();
    assert_eq!(h.menu().find("file.open").unwrap().label, "Öppna");
    assert_eq!(h.menu_rev(), 2);
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

/// P.3d-1: a declared secondary window is its own render pipeline over the
/// SAME reactive store — a signal written through one window's UI re-renders
/// the other on its next pump.
#[test]
fn secondary_window_shares_the_reactive_store() {
    use lumen_core::events::{Event, PointerEvent};

    let app = App::new(|cx| {
        let n = cx.signal("n", || 0i32);
        widgets::text(format!("main sees {}", n.get(cx.runtime()))).id("main-label")
    })
    .window(
        WindowDesc {
            id: "prefs".into(),
            title: "Preferences".into(),
            width: 200.0,
            height: 120.0,
        },
        |cx| {
            let n = cx.signal("n", || 0i32);
            widgets::column(vec![
                widgets::text(format!("prefs sees {}", n.get(cx.runtime()))).id("prefs-label"),
                widgets::button("bump", move |rt| n.update(rt, |v| *v += 1)).id("bump"),
            ])
        },
    );
    let mut main = app.run_headless(Size::new(300.0, 200.0));
    main.pump();
    // The declaration is visible as data (agent: ui.getWindows).
    assert_eq!(main.windows().len(), 1);
    assert_eq!(main.windows()[0].id, "prefs");

    let mut prefs = main.open_window("prefs").expect("declared window opens");
    assert_eq!(prefs.size(), Size::new(200.0, 120.0));

    fn label(h: &Headless, id: &str) -> String {
        fn find(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<String> {
            if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
                return Some(n.label.clone());
            }
            n.children.iter().find_map(|c| find(c, id))
        }
        find(&h.semantics_doc().root.elided(), id).unwrap_or_default()
    }
    assert_eq!(label(&prefs, "prefs-label"), "prefs sees 0");
    assert_eq!(label(&main, "main-label"), "main sees 0");

    // Click the button IN THE PREFS WINDOW (its own hit-testing + input queue).
    let b = {
        fn find(
            n: &lumen_core::semantics::SemanticsNode,
            id: &str,
        ) -> Option<lumen_core::geometry::Rect> {
            if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
                return Some(n.bounds);
            }
            n.children.iter().find_map(|c| find(c, id))
        }
        find(&prefs.semantics_doc().root.elided(), "bump").expect("button in prefs")
    };
    let p = lumen_core::geometry::Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    prefs.inject(Event::PointerDown(PointerEvent::at(p)));
    prefs.inject(Event::PointerUp(PointerEvent::at(p)));
    prefs.pump();
    assert_eq!(label(&prefs, "prefs-label"), "prefs sees 1");

    // The MAIN window sees the same store on its next pump.
    main.pump();
    assert_eq!(label(&main, "main-label"), "main sees 1");

    // Unknown ids don't open.
    assert!(main.open_window("nope").is_none());
}
