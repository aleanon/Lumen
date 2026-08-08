//! T3.5 acceptance (part 2): mobile widgets render, are agent-drivable, and
//! pass the ≥44px touch-target audit.

use kurbo::{Point, Size, Vec2};
use lumen_core::events::{Event, Modifiers, PointerEvent, WheelEvent};
use lumen_core::semantics::{Role, SemanticsNode, State};
use lumen_widgets::audit::audit_touch_targets;
use lumen_widgets::{widgets, App, BuildCx, Element, Headless};

fn run(w: f64, h: f64, build: impl Fn(&mut BuildCx) -> Element + 'static) -> Headless {
    App::new(build).run_headless(Size::new(w, h))
}

fn sem(h: &Headless) -> SemanticsNode {
    h.semantics_doc().root.elided()
}

/// The first node with `role` whose label is exactly `label` — how a calendar
/// day or a dial hour is addressed (they carry a role + label, not an id).
fn by_label<'a>(n: &'a SemanticsNode, role: Role, label: &str) -> Option<&'a SemanticsNode> {
    if n.role == role && n.label == label {
        return Some(n);
    }
    n.children.iter().find_map(|c| by_label(c, role, label))
}

fn by_id<'a>(n: &'a SemanticsNode, id: &str) -> Option<&'a SemanticsNode> {
    if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
        return Some(n);
    }
    n.children.iter().find_map(|c| by_id(c, id))
}

fn click(h: &mut Headless, p: Point) {
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
}

fn center(n: &SemanticsNode) -> Point {
    let b = n.bounds;
    Point::new(b.x0 + b.width() / 2.0, b.y0 + b.height() / 2.0)
}

#[test]
fn bottom_nav_selects_and_is_tappable() {
    let mut h = run(360.0, 80.0, |cx| {
        widgets::bottom_nav(cx, "nav", &["Home", "Search", "Profile"])
    });
    let root = sem(&h);
    assert_eq!(root.role, Role::TabList);
    assert_eq!(root.children.len(), 3);

    // Tap the third item; it becomes selected.
    let third = center(&sem(&h).children[2]);
    click(&mut h, third);
    assert!(sem(&h).children[2].states.contains(&State::Selected));
    assert!(!sem(&h).children[0].states.contains(&State::Selected));

    // Every interactive item is ≥44px.
    assert_eq!(audit_touch_targets(&sem(&h), 44.0), vec![]);
}

#[test]
fn navigation_rail_is_vertical_and_tappable() {
    let h = run(120.0, 360.0, |cx| {
        widgets::navigation_rail(cx, "rail", &["A", "B", "C"])
    });
    assert_eq!(sem(&h).role, Role::TabList);
    // Vertically stacked: each item below the previous.
    let ys: Vec<f64> = sem(&h).children.iter().map(|c| c.bounds.y0).collect();
    assert!(ys[0] < ys[1] && ys[1] < ys[2], "stacked vertically: {ys:?}");
    assert_eq!(audit_touch_targets(&sem(&h), 44.0), vec![]);
}

#[test]
fn app_bar_shows_title_and_actions() {
    let h = run(360.0, 56.0, |cx| {
        let _ = cx;
        widgets::app_bar("Inbox", vec![widgets::button("⋯", |_| {}).id("menu")])
    });
    // W4: the bar carries the title as its own label rather than id-ing the
    // title node `#title` (which would squat on an id apps commonly use).
    let s = sem(&h);
    assert_eq!(s.label, "Inbox", "the bar is labelled by its title");
    assert!(
        by_label(&s, Role::Text, "Inbox").is_some(),
        "and the title text is still in the tree"
    );
    assert!(by_id(&s, "menu").is_some());
}

#[test]
fn pull_to_refresh_triggers_on_overpull() {
    let mut h = run(300.0, 400.0, |cx| {
        let lines: Vec<Element> = (0..5).map(|i| widgets::text(format!("item {i}"))).collect();
        widgets::pull_to_refresh(cx, "feed", 30.0, |_| {}, lines)
    });
    assert_eq!(
        by_id(&sem(&h), "feed-refresh-indicator").unwrap().label,
        "Pull to refresh"
    );

    // Pull down hard at the top (negative wheel delta past threshold).
    let scroll = center(by_id(&sem(&h), "feed-scroll").unwrap());
    h.inject(Event::Wheel(WheelEvent {
        pos: scroll,
        delta: Vec2::new(0.0, -50.0),
        modifiers: Modifiers::empty(),
    }));
    h.pump();
    let s = sem(&h);
    let ind = by_id(&s, "feed-refresh-indicator").unwrap();
    assert_eq!(ind.label, "Refreshing…");
    assert!(ind.states.contains(&State::Busy));
}

#[test]
fn date_picker_selects_a_day_from_the_calendar() {
    // Tall enough for the whole month grid (6 weeks of 44px touch targets).
    let mut h = run(360.0, 420.0, |cx| widgets::date_picker(cx, "dob"));
    let before = by_id(&sem(&h), "dob").unwrap().value.clone().unwrap();
    assert_eq!(before, "2026-06-16");

    // The 16th starts selected; picking the 17th moves the selection.
    let day_17 = center(by_label(&sem(&h), Role::Button, "17").expect("day 17 cell"));
    click(&mut h, day_17);
    assert_eq!(
        by_id(&sem(&h), "dob").unwrap().value.as_deref(),
        Some("2026-06-17")
    );
    assert!(
        by_label(&sem(&h), Role::Button, "17")
            .unwrap()
            .states
            .contains(&State::Selected),
        "the picked day reports itself selected"
    );
    assert_eq!(audit_touch_targets(&sem(&h), 44.0), vec![]);
}

#[test]
fn date_picker_navigates_months() {
    let mut h = run(360.0, 420.0, |cx| widgets::date_picker(cx, "dob"));
    let next = center(by_id(&sem(&h), "dob-date-next").expect("next-month button"));
    click(&mut h, next);
    assert_eq!(
        by_id(&sem(&h), "dob").unwrap().value.as_deref(),
        Some("2026-07-16"),
        "the month advanced, keeping the selected day"
    );
}

#[test]
fn time_picker_value_and_targets() {
    // Tall enough for the digital header + the 240px dial + the minute row.
    let mut h = run(300.0, 400.0, |cx| widgets::time_picker(cx, "alarm"));
    assert_eq!(
        by_id(&sem(&h), "alarm").unwrap().value.as_deref(),
        Some("09:30")
    );
    let min_dec = center(by_id(&sem(&h), "alarm-min-dec").expect("minute decrement"));
    click(&mut h, min_dec);
    assert_eq!(
        by_id(&sem(&h), "alarm").unwrap().value.as_deref(),
        Some("09:29")
    );
    assert_eq!(audit_touch_targets(&sem(&h), 44.0), vec![]);
}

#[test]
fn time_picker_dial_sets_the_hour() {
    let mut h = run(300.0, 400.0, |cx| widgets::time_picker(cx, "alarm"));
    // The dial drives the hour: tap "4 o'clock".
    let four = center(by_id(&sem(&h), "alarm-hour-4").expect("dial hour 4"));
    click(&mut h, four);
    assert_eq!(
        by_id(&sem(&h), "alarm").unwrap().value.as_deref(),
        Some("04:30"),
        "tapping a dial number set the hour"
    );
}
