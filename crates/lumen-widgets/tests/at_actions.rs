//! A11Y2c — an AT can *drive* a virtualized list, not just be told how big it is.
//!
//! `set_size` (A11Y2) tells a screen reader the list holds 100 000 rows, and
//! the keyboard work (A11Y2b) made the control operable. Neither lets an AT
//! address the list *directly*: `route_at_action` handled `Action::Click` and
//! dropped every scroll action AccessKit offers, so an AT could only drive the
//! list by synthesising keystrokes.
//!
//! The decision half is `a11y::resolve_at_action`, which is pure — no window,
//! no adapter — so it can be tested here rather than through a live shell. The
//! shell keeps only the injection.

#![cfg(feature = "accessibility")]

use accesskit::{Action as AkAction, ActionData, ScrollUnit};
use kurbo::{Point, Size, Vec2};
use lumen_core::semantics::{Role, SemanticsNode};
use lumen_widgets::a11y::{resolve_at_action, AtCommand};
use lumen_widgets::{widgets, App, BuildCx, Headless, VirtualList};

const N: usize = 100_000;
const ROW_H: f64 = 24.0;

fn list_app() -> Headless {
    let mut h = App::new(|cx: &mut BuildCx| {
        VirtualList::new(cx, "vl", N, ROW_H, 300.0, |i| {
            widgets::text(format!("Row {i}"))
        })
        .into()
    })
    .run_headless(Size::new(400.0, 300.0));
    h.pump();
    h
}

fn find<'a>(n: &'a SemanticsNode, f: &dyn Fn(&SemanticsNode) -> bool) -> Option<&'a SemanticsNode> {
    if f(n) {
        return Some(n);
    }
    n.children.iter().find_map(|c| find(c, f))
}

fn labels(h: &Headless) -> Vec<String> {
    fn walk(n: &SemanticsNode, out: &mut Vec<String>) {
        if !n.label.is_empty() {
            out.push(n.label.clone());
        }
        n.children.iter().for_each(|c| walk(c, out));
    }
    let mut v = Vec::new();
    walk(&h.semantics_elided(), &mut v);
    v
}

/// The point of the whole exercise: an AT reads `set_size = 100 000`, decides
/// to go to row 50 000, and can actually get there — to a node that does not
/// exist yet and therefore cannot be targeted directly.
#[test]
fn set_scroll_offset_reaches_a_row_that_was_never_in_the_tree() {
    let mut h = list_app();
    let root = h.semantics_elided();
    let list = find(&root, &|n| n.role == Role::List).expect("list");
    assert_eq!(list.set_size, Some(N), "the AT is told the real size");
    assert!(
        !labels(&h).iter().any(|l| l == "Row 50000"),
        "row 50 000 is not in the tree to begin with — it is virtualized"
    );

    let cmd = resolve_at_action(
        &root,
        list.node.fold64(),
        AkAction::SetScrollOffset,
        Some(&ActionData::SetScrollOffset(accesskit::Point::new(
            0.0,
            50_000.0 * ROW_H,
        ))),
    )
    .expect("a scrollable list honours SetScrollOffset");

    let AtCommand::Wheel { pos, delta } = cmd else {
        panic!("expected a wheel command, got {cmd:?}");
    };
    h.inject(lumen_core::events::Event::Wheel(
        lumen_core::events::WheelEvent {
            pos,
            delta,
            modifiers: lumen_core::events::Modifiers::empty(),
        },
    ));
    h.pump();

    assert!(
        labels(&h).iter().any(|l| l == "Row 50000"),
        "the AT jumped to row 50 000 and it materialized. Got: {:?}",
        &labels(&h)[..labels(&h).len().min(4)]
    );
}

#[test]
fn scroll_down_moves_a_line_and_a_page_moves_a_page() {
    let h = list_app();
    let root = h.semantics_elided();
    let list = find(&root, &|n| n.role == Role::List).expect("list");
    let id = list.node.fold64();

    let line = match resolve_at_action(&root, id, AkAction::ScrollDown, None).unwrap() {
        AtCommand::Wheel { delta, .. } => delta.y,
        c => panic!("{c:?}"),
    };
    assert_eq!(
        line,
        lumen_core::events::WHEEL_LINE_PX,
        "an absent ScrollUnit means a line, and down is positive"
    );

    let page = match resolve_at_action(
        &root,
        id,
        AkAction::ScrollDown,
        Some(&ActionData::ScrollUnit(ScrollUnit::Page)),
    )
    .unwrap()
    {
        AtCommand::Wheel { delta, .. } => delta.y,
        c => panic!("{c:?}"),
    };
    assert!(page > line, "a page is more than a line: {page} vs {line}");

    let up = match resolve_at_action(&root, id, AkAction::ScrollUp, None).unwrap() {
        AtCommand::Wheel { delta, .. } => delta.y,
        c => panic!("{c:?}"),
    };
    assert_eq!(up, -line, "up is the negation of down");
}

/// `ScrollIntoView` is sent to the node to *reveal*; the thing that has to move
/// is its ancestor. A resolver that scrolled the target itself would do nothing
/// for a list row, which is the only case that matters.
#[test]
fn scroll_into_view_drives_the_ancestor_not_the_target() {
    let h = list_app();
    let root = h.semantics_elided();
    let list = find(&root, &|n| n.role == Role::List).expect("list");
    // A row near the bottom of the overscan, partly below the viewport.
    let row = list
        .children
        .iter()
        .flat_map(|c| c.children.iter().chain(std::iter::once(c)))
        .filter(|n| n.position_in_set.is_some())
        .max_by(|a, b| a.bounds.y1.total_cmp(&b.bounds.y1))
        .expect("some row");
    assert!(
        row.bounds.y1 > list.bounds.y1,
        "the chosen row really does hang below the viewport ({} vs {})",
        row.bounds.y1,
        list.bounds.y1
    );

    let cmd = resolve_at_action(&root, row.node.fold64(), AkAction::ScrollIntoView, None)
        .expect("a row inside a scroller can be revealed");
    let AtCommand::Wheel { pos, delta } = cmd else {
        panic!("{cmd:?}")
    };
    assert!(
        delta.y > 0.0,
        "revealing a row below the fold scrolls forward, got {delta:?}"
    );
    assert!(
        list.bounds.contains(pos),
        "the wheel is aimed at the list, not the row"
    );
}

#[test]
fn revealing_something_already_visible_does_not_move_the_view() {
    let h = list_app();
    let root = h.semantics_elided();
    let list = find(&root, &|n| n.role == Role::List).expect("list");
    let row = list
        .children
        .iter()
        .flat_map(|c| c.children.iter().chain(std::iter::once(c)))
        .find(|n| n.position_in_set == Some(1))
        .expect("row 1");
    let cmd = resolve_at_action(&root, row.node.fold64(), AkAction::ScrollIntoView, None).unwrap();
    let AtCommand::Wheel { delta, .. } = cmd else {
        panic!("{cmd:?}")
    };
    assert_eq!(
        delta,
        Vec2::ZERO,
        "an AT revealing what is already on screen must not jolt the view"
    );
}

/// `None` means "do nothing" and must never mean "guess".
#[test]
fn unresolvable_requests_are_refused() {
    let h = list_app();
    let root = h.semantics_elided();
    let list = find(&root, &|n| n.role == Role::List).expect("list");
    let id = list.node.fold64();

    assert!(
        resolve_at_action(&root, 0xDEAD_BEEF, AkAction::Click, None).is_none(),
        "an unknown target resolves to nothing"
    );
    assert!(
        resolve_at_action(&root, id, AkAction::SetScrollOffset, None).is_none(),
        "SetScrollOffset without its data is refused, not defaulted to zero"
    );
    assert!(
        resolve_at_action(
            &root,
            id,
            AkAction::SetScrollOffset,
            Some(&ActionData::ScrollUnit(ScrollUnit::Page))
        )
        .is_none(),
        "…and mistyped data is refused too"
    );
    assert!(
        resolve_at_action(&root, id, AkAction::ScrollIntoView, None).is_none(),
        "the list has no scrollable ancestor, so it cannot be revealed"
    );
    assert!(
        resolve_at_action(&root, id, AkAction::Increment, None).is_none(),
        "an action this resolver does not implement is refused"
    );
}

/// Click must keep working — it was the only action before this change.
#[test]
fn click_still_resolves_to_the_node_centre() {
    let h = list_app();
    let root = h.semantics_elided();
    let list = find(&root, &|n| n.role == Role::List).expect("list");
    let cmd = resolve_at_action(&root, list.node.fold64(), AkAction::Click, None).unwrap();
    assert_eq!(
        cmd,
        AtCommand::Click(Point::new(
            (list.bounds.x0 + list.bounds.x1) / 2.0,
            (list.bounds.y0 + list.bounds.y1) / 2.0
        ))
    );
}

/// Declaring an action an AT cannot use is a lie it will act on: it reports a
/// control the user then cannot operate.
#[test]
fn only_the_axes_that_can_move_are_advertised() {
    let h = list_app();
    let root = h.semantics_elided();
    let update = lumen_widgets::a11y::build_tree(&root);
    let list = find(&root, &|n| n.role == Role::List).expect("list");
    let want = accesskit::NodeId(list.node.fold64());
    let (_, node) = update
        .nodes
        .iter()
        .find(|(id, _)| *id == want)
        .expect("the list is published");

    assert!(node.supports_action(AkAction::ScrollDown));
    assert!(node.supports_action(AkAction::ScrollUp));
    assert!(
        node.supports_action(AkAction::SetScrollOffset),
        "the action that makes 100 000 rows reachable"
    );
    assert!(
        !node.supports_action(AkAction::ScrollLeft),
        "a vertical-only list must not advertise horizontal scrolling"
    );
    assert!(!node.supports_action(AkAction::ScrollRight));
}
