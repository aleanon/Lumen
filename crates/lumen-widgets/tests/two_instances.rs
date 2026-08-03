//! W4 — two of the same widget on one screen must not collide.
//!
//! Several widgets hardcoded their child ids (`Stepper` used `dec`/`inc`/
//! `value`, the pickers `date-prev`/`hour-4`/`min-dec`, `PullToRefresh`
//! `scroll`). Two instances then produced duplicate `StableId`s: `W0001` fires,
//! selectors become ambiguous, and "first match wins" means the agent silently
//! drives the wrong widget. Two steppers on one screen is not an exotic case.
//!
//! These tests put two of each on a screen and drive the *second* one, which is
//! exactly what a first-match-wins collision breaks.

use kurbo::Size;
use lumen_core::semantics::SemanticsNode;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, widgets_m1, widgets_m3, App, BuildCx, Element, Headless};

fn sem(h: &Headless) -> SemanticsNode {
    h.semantics_doc().root.elided()
}

/// Every stable id in the tree, in document order (duplicates included).
fn all_ids(n: &SemanticsNode, out: &mut Vec<String>) {
    if let Some(i) = &n.id {
        out.push(i.as_str().to_string());
    }
    for c in &n.children {
        all_ids(c, out);
    }
}

fn assert_ids_unique(h: &Headless, what: &str) {
    let mut ids = Vec::new();
    all_ids(&sem(h), &mut ids);
    let mut sorted = ids.clone();
    sorted.sort();
    let before = sorted.len();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        before,
        "two {what} produced duplicate ids (W0001; selectors become ambiguous): {ids:?}"
    );
}

#[test]
fn two_steppers_are_independent() {
    let mut h = App::new(|cx: &mut BuildCx| {
        widgets::column(vec![
            widgets_m1::Stepper::new(cx, "qty-a", 0, 10).into(),
            widgets_m1::Stepper::new(cx, "qty-b", 0, 10).into(),
        ])
    })
    .run_headless(Size::new(320.0, 200.0));
    h.pump();
    assert_ids_unique(&h, "steppers");

    // Drive the SECOND one — a first-match-wins collision would move the first.
    h.invoke_action("#qty-b-inc", "click")
        .expect("second stepper");
    h.invoke_action("#qty-b-inc", "click").unwrap();

    let a: Signal<i64> = h.runtime().signal("qty-a", || 0i64);
    let b: Signal<i64> = h.runtime().signal("qty-b", || 0i64);
    assert_eq!(b.get(h.runtime()), 2, "the addressed stepper moved");
    assert_eq!(a.get(h.runtime()), 0, "its neighbour did not");
}

#[test]
fn two_date_pickers_are_independent() {
    let mut h = App::new(|cx: &mut BuildCx| {
        widgets::column(vec![
            widgets_m3::DatePicker::new(cx, "from").into(),
            widgets_m3::DatePicker::new(cx, "to").into(),
        ])
    })
    .run_headless(Size::new(360.0, 900.0));
    h.pump();
    assert_ids_unique(&h, "date pickers");

    h.invoke_action("#to-date-next", "click")
        .expect("the second picker's next-month button");

    let from_m: Signal<i64> = h.runtime().signal("from.month", || 6i64);
    let to_m: Signal<i64> = h.runtime().signal("to.month", || 6i64);
    assert_eq!(to_m.get(h.runtime()), 7, "the addressed picker advanced");
    assert_eq!(from_m.get(h.runtime()), 6, "its neighbour did not");
}

#[test]
fn two_time_pickers_are_independent() {
    let mut h = App::new(|cx: &mut BuildCx| {
        widgets::column(vec![
            widgets_m3::TimePicker::new(cx, "start").into(),
            widgets_m3::TimePicker::new(cx, "end").into(),
        ])
    })
    .run_headless(Size::new(320.0, 900.0));
    h.pump();
    assert_ids_unique(&h, "time pickers");

    h.invoke_action("#end-hour-4", "click")
        .expect("the second dial's hour 4");
    let start_h: Signal<i64> = h.runtime().signal("start.hour", || 9i64);
    let end_h: Signal<i64> = h.runtime().signal("end.hour", || 9i64);
    assert_eq!(end_h.get(h.runtime()), 4, "the addressed dial set its hour");
    assert_eq!(start_h.get(h.runtime()), 9, "its neighbour did not");
}

#[test]
fn two_dropdowns_are_independent() {
    let mut h = App::new(|cx: &mut BuildCx| {
        widgets::column(vec![
            lumen_widgets::PickList::new(cx, "a", "Pick…", ["x", "y"]).into(),
            lumen_widgets::PickList::new(cx, "b", "Pick…", ["x", "y"]).into(),
        ])
    })
    .run_headless(Size::new(300.0, 300.0));
    h.pump();
    assert_ids_unique(&h, "dropdowns");

    h.invoke_action("#b-trigger", "click")
        .expect("the second dropdown's trigger");
    let a_open: Signal<bool> = h.runtime().signal("a.open", || false);
    let b_open: Signal<bool> = h.runtime().signal("b.open", || false);
    assert!(b_open.get(h.runtime()), "the addressed dropdown opened");
    assert!(!a_open.get(h.runtime()), "its neighbour stayed closed");
}

#[test]
fn two_tab_bars_are_independent() {
    let mut h = App::new(|cx: &mut BuildCx| {
        widgets::column(vec![
            widgets_m1::Tabs::new(cx, "top", &["A", "B"]).into(),
            widgets_m1::Tabs::new(cx, "bottom", &["A", "B"]).into(),
        ])
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    assert_ids_unique(&h, "tab bars");

    h.invoke_action("#bottom-tab-1", "click")
        .expect("second bar");
    let top: Signal<usize> = h.runtime().signal("top", || 0usize);
    let bottom: Signal<usize> = h.runtime().signal("bottom", || 0usize);
    assert_eq!(bottom.get(h.runtime()), 1, "the addressed bar switched");
    assert_eq!(top.get(h.runtime()), 0, "its neighbour did not");
}

/// An app is free to use `#title` for its own heading; the framework must not
/// squat on it. `AppBar` used to id its title node `title` unconditionally.
#[test]
fn an_app_bar_does_not_squat_on_a_common_id() {
    let mut h = App::new(|_cx: &mut BuildCx| {
        widgets::column(vec![
            widgets_m3::AppBar::new("Inbox", vec![]).into(),
            widgets::text("My heading").id("title"),
        ])
    })
    .run_headless(Size::new(360.0, 200.0));
    h.pump();

    let mut ids = Vec::new();
    all_ids(&sem(&h), &mut ids);
    assert_eq!(
        ids.iter().filter(|i| *i == "title").count(),
        1,
        "the app's own #title must be the only one: {ids:?}"
    );
}

/// The general net: a screen of two of everything stays unambiguous.
#[test]
fn a_screen_of_paired_widgets_has_no_duplicate_ids() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let mut kids: Vec<Element> = Vec::new();
        for tag in ["one", "two"] {
            kids.push(widgets_m1::Stepper::new(cx, &format!("{tag}-n"), 0, 5).into());
            kids.push(widgets_m1::Tabs::new(cx, &format!("{tag}-t"), &["A", "B"]).into());
            kids.push(lumen_widgets::PickList::new(cx, &format!("{tag}-p"), "…", ["x"]).into());
            kids.push(widgets_m3::DatePicker::new(cx, &format!("{tag}-d")).into());
            kids.push(widgets_m3::TimePicker::new(cx, &format!("{tag}-tm")).into());
        }
        widgets::column(kids)
    })
    .run_headless(Size::new(420.0, 2000.0));
    h.pump();
    assert_ids_unique(&h, "paired widgets");
}
