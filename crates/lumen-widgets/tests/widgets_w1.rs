//! W.1 (docs/plan-remediation-2026-07.md): Popover, Sheet, Drawer,
//! SearchField, plus the promoted Toast/Spinner/Chip. Headless behavior per
//! the writing-widgets pattern: open/close/dismiss round-trips, semantics
//! visible to the agent, state in signals.

use kurbo::{Point, Size};
use lumen_core::events::{Event, PointerEvent};
use lumen_core::state::Signal;
use lumen_widgets::{
    center, col, widgets, App, BuildCx, Chip, Drawer, Element, Popover, SearchField, Sheet,
    Spinner, Toast, ToastKind,
};

fn find_label(n: &lumen_core::semantics::SemanticsNode, label: &str) -> Option<kurbo::Rect> {
    if n.label == label {
        return Some(n.bounds);
    }
    n.children.iter().find_map(|c| find_label(c, label))
}

fn click(h: &mut lumen_widgets::Headless, id: &str) {
    let p = center(h.node_bounds_by_id(id).unwrap());
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
}

#[test]
fn popover_opens_closes_and_dismisses() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let trigger: Element = widgets::button("Open", |_| {}).id("trig");
        let content = widgets::text("popover content").id("content");
        col![
            Popover::new(cx, "pop", trigger, content).id("pop"),
            widgets::button("elsewhere", |_| {}).id("other")
        ]
    })
    .run_headless(Size::new(400.0, 300.0));
    h.pump();
    assert!(h.node_bounds_by_id("content").is_none(), "closed initially");

    click(&mut h, "trig");
    let b = h.node_bounds_by_id("content").expect("open after click");
    let t = h.node_bounds_by_id("trig").unwrap();
    assert!(b.y0 >= t.y1, "panel sits below the trigger: {t:?} -> {b:?}");

    // Escape light-dismisses.
    h.inject(Event::KeyDown(lumen_core::events::KeyEvent {
        key: lumen_core::events::Key::Named(lumen_core::events::NamedKey::Escape),
        modifiers: lumen_core::events::Modifiers::empty(),
        repeat: false,
    }));
    h.pump();
    assert!(h.node_bounds_by_id("content").is_none(), "Escape closed it");
    h.assert_view_coherent();
}

#[test]
fn sheet_and_drawer_open_via_signal_and_scrim_close() {
    let mut h = App::new(|cx: &mut BuildCx| {
        cx.signal("s.open", || false);
        cx.signal("d.open", || false);
        col![
            Sheet::new(cx, "s", widgets::text("sheet body").id("sheet-body")).id("sheet"),
            Drawer::new(cx, "d", widgets::text("drawer body").id("drawer-body")).id("drawer"),
            widgets::button("app", |_| {}).id("app-btn")
        ]
    })
    .run_headless(Size::new(500.0, 400.0));
    h.pump();
    assert!(h.node_bounds_by_id("sheet-body").is_none());
    assert!(h.node_bounds_by_id("drawer-body").is_none());

    let s: Signal<bool> = h.runtime().signal("s.open", || false);
    s.set(h.runtime(), true);
    h.pump();
    let b = h.node_bounds_by_id("sheet-body").expect("sheet open");
    assert!(
        b.y1 > 300.0,
        "sheet panel anchors to the bottom of the 400px window: {b:?}"
    );
    // Click the scrim (top-left corner is scrim, not panel) closes it.
    h.inject(Event::PointerDown(PointerEvent::at(Point::new(5.0, 5.0))));
    h.inject(Event::PointerUp(PointerEvent::at(Point::new(5.0, 5.0))));
    h.pump();
    assert!(h.node_bounds_by_id("sheet-body").is_none(), "scrim closed");

    let d: Signal<bool> = h.runtime().signal("d.open", || false);
    d.set(h.runtime(), true);
    h.pump();
    let b = h.node_bounds_by_id("drawer-body").expect("drawer open");
    assert!(b.x0 < 300.0, "drawer panel anchors left: {b:?}");
    h.assert_view_coherent();
}

#[test]
fn search_field_types_and_clears() {
    let mut h = App::new(|cx: &mut BuildCx| col![SearchField::new(cx, "q", "Search…").id("sf")])
        .run_headless(Size::new(400.0, 200.0));
    h.pump();

    // Type into the field (click focuses the inner editor).
    let b = h.node_bounds_by_id("sf").unwrap();
    let p = center(b);
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.inject(Event::TextInput(lumen_core::events::TextInputEvent {
        text: "hello".into(),
    }));
    h.pump();
    let sem = h.semantics_json().to_string();
    assert!(sem.contains("hello"), "typed text visible: {sem}");
    assert!(
        sem.contains("clear search"),
        "clear affordance appears with text"
    );

    // The clear button empties the editor.
    let root = h.semantics_doc().root.elided();
    let cb = find_label(&root, "clear search").expect("clear button present");
    h.inject(Event::PointerDown(PointerEvent::at(center(cb))));
    h.inject(Event::PointerUp(PointerEvent::at(center(cb))));
    h.pump();
    let sem = h.semantics_json().to_string();
    assert!(!sem.contains("hello"), "cleared: {sem}");
    h.assert_view_coherent();
}

#[test]
fn toast_spinner_chip_render_with_semantics() {
    let mut h = App::new(|cx: &mut BuildCx| {
        col![
            Toast::new(ToastKind::Success, "Saved", "All good.").id("toast"),
            Spinner::new(cx, 48.0).id("spin"),
            Chip::new("rust").on_remove(|_| {}).id("chip")
        ]
    })
    .run_headless(Size::new(500.0, 400.0));
    h.pump();

    let sem = h.semantics_json().to_string();
    assert!(sem.contains("Saved") && sem.contains("All good."), "{sem}");
    assert!(sem.contains("loading"), "spinner is labelled: {sem}");
    assert!(sem.contains("rust") && sem.contains("remove"), "{sem}");
    assert!(h.is_time_driven(), "spinner animates (continuous)");
    // No tofu anywhere (the chip's × is a real glyph).
    assert!(
        !h.lint().iter().any(|d| d.code == "W0402"),
        "{:?}",
        h.lint()
    );
    h.assert_view_coherent();
}
