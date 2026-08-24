//! Clicking a toggling trigger closes its overlay — and leaves it closed.
//!
//! Light dismiss runs on the *press* and tests the press against the overlay's
//! bounds. A trigger sits outside the panel, so the press dismissed the panel
//! and the release then ran the trigger's toggle and reopened it: an open
//! dropdown collapsed and instantly re-expanded, looking like the click did
//! nothing. The press is now tested against the overlay's owner — its direct
//! parent, which is the wrapper holding the trigger.

use kurbo::{Rect, Size};
use lumen_core::events::{Event, Key, KeyEvent, Modifiers, NamedKey, PointerEvent};
use lumen_core::geometry::Point;
use lumen_core::semantics::SemanticsNode;
use lumen_core::state::Signal;
use lumen_widgets::{col, widgets, App, BuildCx, Element, Headless, PickList, Popover, Sheet};

fn rect_id(n: &SemanticsNode, id: &str) -> Option<Rect> {
    if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
        return Some(n.bounds);
    }
    n.children.iter().find_map(|c| rect_id(c, id))
}

fn click_at(h: &mut Headless, p: Point) {
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
}

fn click(h: &mut Headless, id: &str) {
    let b = rect_id(&h.semantics_doc().root, id).unwrap_or_else(|| panic!("no #{id}"));
    click_at(h, Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0));
}

fn open_flag(h: &Headless, key: &str) -> bool {
    let s: Signal<bool> = h.runtime().signal(key, || false);
    s.get(h.runtime())
}

fn picker() -> Headless {
    let mut h = App::new(|cx: &mut BuildCx| {
        col![PickList::new(cx, "p", "Pick…", ["one", "two", "three"]).id("p")]
    })
    .run_headless(Size::new(400.0, 400.0));
    h.pump();
    h
}

#[test]
fn clicking_an_open_trigger_closes_it_and_it_stays_closed() {
    let mut h = picker();
    click(&mut h, "p-trigger");
    assert!(open_flag(&h, "p.open"), "first click opens");
    click(&mut h, "p-trigger");
    assert!(
        !open_flag(&h, "p.open"),
        "a second click on the trigger closes it — it must not re-open in the same gesture"
    );
    click(&mut h, "p-trigger");
    assert!(open_flag(&h, "p.open"), "and a third opens it again");
}

#[test]
fn clicking_away_still_dismisses() {
    let mut h = picker();
    click(&mut h, "p-trigger");
    assert!(open_flag(&h, "p.open"));
    click_at(&mut h, Point::new(380.0, 380.0));
    assert!(!open_flag(&h, "p.open"), "a press outside dismisses");
}

#[test]
fn escape_still_dismisses() {
    let mut h = picker();
    click(&mut h, "p-trigger");
    h.inject(Event::KeyDown(KeyEvent {
        key: Key::Named(NamedKey::Escape),
        modifiers: Modifiers::empty(),
        repeat: false,
    }));
    h.pump();
    assert!(!open_flag(&h, "p.open"));
}

/// The same shape, through `Popover` — which `Menu::button` and every anchored
/// panel are built on.
#[test]
fn a_popover_trigger_toggles_cleanly() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let trigger = widgets::button("Open", |_| {}).id("t");
        col![Popover::new(cx, "pop", trigger, widgets::text("panel")).id("pop")]
    })
    .run_headless(Size::new(400.0, 400.0));
    h.pump();
    click(&mut h, "t");
    assert!(open_flag(&h, "pop.open"));
    click(&mut h, "t");
    assert!(
        !open_flag(&h, "pop.open"),
        "the trigger closes its own panel"
    );
}

/// A `Sheet` lives inside a full-window wrapper, so light dismiss never fires
/// for it from a press — its scrim closes it, which correctly leaves a press on
/// the panel alone.
#[test]
fn a_sheet_closes_on_the_scrim_but_not_on_its_panel() {
    let mut h = App::new(|cx: &mut BuildCx| {
        Sheet::new(cx, "sh", widgets::text("sheet body").id("body")).into()
    })
    .run_headless(Size::new(400.0, 400.0));
    h.pump();
    let open: Signal<bool> = h.runtime().signal("sh.open", || false);
    open.set(h.runtime(), true);
    h.pump();

    // A press on the panel leaves it open.
    click(&mut h, "body");
    assert!(
        open_flag(&h, "sh.open"),
        "the panel is not a dismiss target"
    );

    // A press on the scrim (top of the window; the sheet is bottom-anchored)
    // closes it.
    click_at(&mut h, Point::new(200.0, 20.0));
    assert!(!open_flag(&h, "sh.open"), "the scrim closes the sheet");
}

/// A disabled control does not advertise a pointer shape: the cursor is a
/// promise about what a click will do, and the answer is nothing.
#[test]
fn a_disabled_control_shows_no_hand() {
    use lumen_widgets::Button;
    let mut h = App::new(|_cx: &mut BuildCx| {
        col![
            Button::new("live").on_press(|_| {}).id("live"),
            Button::new("dead")
                .on_press(|_| {})
                .disabled(true)
                .id("dead"),
        ]
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();

    let hover = |h: &mut Headless, id: &str| {
        let b = h.node_bounds_by_id(id).unwrap();
        h.inject(Event::PointerMove(PointerEvent::at(Point::new(
            (b.x0 + b.x1) / 2.0,
            (b.y0 + b.y1) / 2.0,
        ))));
        h.pump();
    };
    hover(&mut h, "live");
    assert_eq!(h.cursor_name(), "pointer");
    hover(&mut h, "dead");
    assert_eq!(
        h.cursor_name(),
        "default",
        "a disabled button offers nothing, so it promises nothing"
    );
}

/// The pointer shape follows whatever the pointer is actually over.
///
/// Declaring a `cursor` is declaring that the pointer means something on this
/// node, so such a node takes part in hit-testing — otherwise a purely
/// decorative element like a pane divider can never be *hovered*, and the shape
/// it declares is unreachable. Presses still bubble to the interactive
/// ancestor, so the divider's grab is unaffected.
#[test]
fn the_cursor_tracks_the_node_under_the_pointer() {
    use lumen_widgets::{Button, PaneGrid};
    let mut h = App::new(|cx: &mut BuildCx| {
        let mut pg: Element = PaneGrid::new(
            cx,
            "pg",
            widgets::text("left").id("left"),
            widgets::text("right").id("right"),
        )
        .into();
        pg.style.height = lumen_layout::Dim::px(80.0);
        col![
            Button::new("go").on_press(|_| {}).id("go"),
            widgets::text("plain").id("plain"),
            pg,
        ]
    })
    .run_headless(Size::new(320.0, 240.0));
    h.pump();

    let hover = |h: &mut Headless, id: &str| {
        let b = h
            .node_bounds_by_id(id)
            .unwrap_or_else(|| panic!("no #{id}"));
        h.inject(Event::PointerMove(PointerEvent::at(Point::new(
            (b.x0 + b.x1) / 2.0,
            (b.y0 + b.y1) / 2.0,
        ))));
        h.pump();
    };

    hover(&mut h, "go");
    assert_eq!(h.cursor_name(), "pointer");
    hover(&mut h, "plain");
    assert_eq!(
        h.cursor_name(),
        "default",
        "the hand does not follow you off the button"
    );
    hover(&mut h, "pg-divider");
    assert_eq!(
        h.cursor_name(),
        "col-resize",
        "the divider says it is draggable"
    );
    hover(&mut h, "plain");
    assert_eq!(h.cursor_name(), "default");
}
