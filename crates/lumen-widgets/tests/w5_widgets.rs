//! W5 — the remaining capability fills: indeterminate progress, avatar images,
//! selectable chips, and toasts that behave like toasts.

use kurbo::Size;
use lumen_core::semantics::{Role, SemanticsNode, State as SemState};
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App, Avatar, BuildCx, Chip, Headless, ProgressBar, Toast, ToastKind};

fn sem(h: &Headless) -> SemanticsNode {
    h.semantics_doc().root.elided()
}

fn by_role(n: &SemanticsNode, r: Role) -> Option<&SemanticsNode> {
    if n.role == r {
        return Some(n);
    }
    n.children.iter().find_map(|c| by_role(c, r))
}

/// The most common progress case — "working, duration unknown" — had no answer
/// (`Spinner` is indeterminate but a different shape). An indeterminate bar must
/// not claim a percentage it doesn't know.
#[test]
fn an_indeterminate_bar_reports_busy_and_no_percentage() {
    let mut h = App::new(|cx: &mut BuildCx| ProgressBar::indeterminate(cx).id("p").into())
        .run_headless(Size::new(300.0, 60.0));
    h.pump();

    let tree = sem(&h);
    let bar = by_role(&tree, Role::Progress).expect("a progress node");
    assert!(
        bar.value.is_none(),
        "an indeterminate bar must not publish a percentage, got {:?}",
        bar.value
    );
    assert!(
        bar.states.contains(&SemState::Busy),
        "it should report Busy so AT announces work in progress"
    );
}

/// It animates off the virtual clock, so it is deterministic — and the sweep
/// actually moves.
#[test]
fn an_indeterminate_bar_sweeps_with_the_clock() {
    let mut h = App::new(|cx: &mut BuildCx| ProgressBar::indeterminate(cx).id("p").into())
        .run_headless(Size::new(300.0, 60.0));
    h.pump();
    let first = h.screenshot();

    h.advance_clock(400.0);
    h.pump();
    let later = h.screenshot();
    assert!(
        first != later,
        "the sweep should have moved after 400ms of clock"
    );

    // Determinism: the same clock position renders the same frame.
    let mut g = App::new(|cx: &mut BuildCx| ProgressBar::indeterminate(cx).id("p").into())
        .run_headless(Size::new(300.0, 60.0));
    g.pump();
    g.advance_clock(400.0);
    g.pump();
    assert!(
        g.screenshot() == later,
        "the same virtual clock must produce the same frame"
    );
}

/// A determinate bar is unaffected — it still reports its percentage.
#[test]
fn a_determinate_bar_still_reports_its_percentage() {
    let mut h = App::new(|_cx: &mut BuildCx| ProgressBar::new(0.65).id("p").into())
        .run_headless(Size::new(300.0, 60.0));
    h.pump();
    let tree = sem(&h);
    let bar = by_role(&tree, Role::Progress).unwrap();
    assert_eq!(bar.value.as_deref(), Some("65%"));
}

/// An avatar takes an image with initials as the *fallback* — the contract every
/// toolkit uses. Before, it could only ever draw initials while claiming
/// `Role::Image`.
#[test]
fn an_avatar_can_show_an_image_and_keeps_its_label() {
    let img = lumen_render::RgbaImage::from_raw(32, 32, vec![0x40; 32 * 32 * 4]);
    let mut h = App::new(move |_cx: &mut BuildCx| {
        Avatar::new("Ada Lovelace", 40.0)
            .image(img.clone())
            .id("a")
            .into()
    })
    .run_headless(Size::new(120.0, 100.0));
    h.pump();

    let tree = sem(&h);
    let a = by_role(&tree, Role::Image).expect("avatar node");
    assert_eq!(
        a.label, "Ada Lovelace",
        "the accessible label stays the person, not the file"
    );
    assert!(h.node_bounds_by_id("a").is_some());
    h.assert_view_coherent();
}

#[test]
fn an_avatar_without_an_image_still_draws_initials() {
    let mut h = App::new(|_cx: &mut BuildCx| Avatar::new("Ada Lovelace", 40.0).id("a").into())
        .run_headless(Size::new(120.0, 100.0));
    h.pump();
    let dump = h.semantics_json().to_string();
    assert!(dump.contains("AL"), "initials are the fallback: {dump}");
}

/// Filter/choice chips need selection — visually *and* semantically, so the
/// agent can tell which filters are on.
#[test]
fn a_chip_can_be_selected_and_says_so() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let on = cx.signal("on", || false);
        let v = on.get(cx.runtime());
        widgets::row(vec![Chip::new("Unread")
            .selected(v, move |rt| on.update(rt, |b| *b = !*b))
            .id("c")
            .into()])
    })
    .run_headless(Size::new(200.0, 80.0));
    h.pump();

    let unselected = sem(&h);
    let c = unselected.children.first().expect("chip present");
    assert!(!c.states.contains(&SemState::Selected), "starts unselected");

    h.invoke_action("#c", "click")
        .expect("a selectable chip acts");
    let after = sem(&h);
    let c = after.children.first().unwrap();
    assert!(
        c.states.contains(&SemState::Selected),
        "clicking selects it, and the state is published"
    );
    assert!(h.audit_actions().is_empty(), "{:?}", h.audit_actions());
}

/// A toast without a timeout is a banner. Expiry is a pure function of the
/// virtual clock, so this test advances time instead of sleeping.
#[test]
fn a_toast_dismisses_itself_after_its_timeout() {
    let mut h = App::new(|cx: &mut BuildCx| {
        Toast::new(ToastKind::Info, "Saved", "Your changes are stored")
            .auto_dismiss(cx, "t", 3_000.0)
            .id("toast")
            .into()
    })
    .run_headless(Size::new(400.0, 140.0));
    h.pump();
    assert!(
        by_role(&sem(&h), Role::Alert).is_some(),
        "the toast is up to begin with"
    );

    h.advance_clock(2_000.0);
    h.pump();
    assert!(
        by_role(&sem(&h), Role::Alert).is_some(),
        "still up before the timeout"
    );

    h.advance_clock(1_500.0);
    h.pump();
    assert!(
        by_role(&sem(&h), Role::Alert).is_none(),
        "gone once the timeout passes"
    );
}

#[test]
fn a_toast_can_carry_an_action() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let undone = cx.signal("undone", || false);
        Toast::new(ToastKind::Info, "Deleted", "1 item removed")
            .action("Undo", move |rt| undone.set(rt, true))
            .id("toast")
            .into()
    })
    .run_headless(Size::new(400.0, 140.0));
    h.pump();

    h.invoke_action(r#"button:text("Undo")"#, "click")
        .expect("the toast action is actuable");
    let undone: Signal<bool> = h.runtime().signal("undone", || false);
    assert!(undone.get(h.runtime()), "the action ran");
}
