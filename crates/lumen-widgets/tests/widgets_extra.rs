//! T4.5: the remaining widget set renders, exposes semantics, and reacts.

use kurbo::{Point, Size};
use lumen_core::events::{Event, PointerEvent};
use lumen_core::semantics::{Role, SemanticsNode, State};
use lumen_widgets::{widgets, widgets_extra, App, BuildCx, Element, Headless};

fn run(build: impl Fn(&mut BuildCx) -> Element + 'static) -> Headless {
    App::new(build).run_headless(Size::new(300.0, 240.0))
}
fn sem(h: &Headless) -> SemanticsNode {
    h.semantics_doc().root.elided()
}
fn by_role(n: &SemanticsNode, role: Role) -> Option<SemanticsNode> {
    if n.role == role {
        return Some(n.clone());
    }
    n.children.iter().find_map(|c| by_role(c, role))
}
fn count(n: &SemanticsNode, role: Role) -> usize {
    usize::from(n.role == role) + n.children.iter().map(|c| count(c, role)).sum::<usize>()
}
fn click(h: &mut Headless, p: Point) {
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
}
fn mid(n: &SemanticsNode) -> Point {
    let b = n.bounds;
    Point::new(b.x0 + b.width() / 2.0, b.y0 + b.height() / 2.0)
}

#[test]
fn radio_group_is_single_select() {
    let mut h = run(|cx| {
        widgets::column(vec![
            widgets_extra::radio(cx, "g", 0, "A").id("a"),
            widgets_extra::radio(cx, "g", 1, "B").id("b"),
        ])
    });
    // A checked by default (group = 0), B unchecked.
    assert!(by_role(&sem(&h), Role::Radio)
        .unwrap()
        .states
        .contains(&State::Checked));
    // Select B; now exactly one is checked.
    let b = mid(&sem(&h).children[1]);
    click(&mut h, b);
    let radios = {
        let mut v = vec![];
        fn collect(n: &SemanticsNode, v: &mut Vec<SemanticsNode>) {
            if n.role == Role::Radio {
                v.push(n.clone());
            }
            for c in &n.children {
                collect(c, v);
            }
        }
        collect(&sem(&h), &mut v);
        v
    };
    let checked = radios
        .iter()
        .filter(|r| r.states.contains(&State::Checked))
        .count();
    assert_eq!(checked, 1, "exactly one radio checked");
    assert!(
        radios[1].states.contains(&State::Checked),
        "B is now checked"
    );
}

#[test]
fn select_cycles_options() {
    let mut h = run(|cx| widgets_extra::select(cx, "s", &["Red", "Green", "Blue"]));
    assert_eq!(sem(&h).value.as_deref(), Some("Red"));
    let p = mid(&sem(&h));
    click(&mut h, p);
    assert_eq!(sem(&h).value.as_deref(), Some("Green"));
}

#[test]
fn tooltip_menu_grid_wrap_split_textarea_render() {
    let h = run(|_| widgets_extra::tooltip(widgets::text("hover me"), "the tip"));
    assert!(by_role(&sem(&h), Role::Tooltip).is_some());

    let h = run(|_| widgets_extra::menu(&["New", "Open", "Save"]));
    assert_eq!(sem(&h).role, Role::Menu);
    assert_eq!(count(&sem(&h), Role::MenuItem), 3);

    let h =
        run(|_| widgets_extra::grid(3, (0..6).map(|i| widgets::text(format!("c{i}"))).collect()));
    assert_eq!(sem(&h).children.len(), 6);

    let h = run(|_| widgets_extra::wrap((0..4).map(|i| widgets::text(format!("w{i}"))).collect()));
    assert_eq!(sem(&h).children.len(), 4);

    let h = run(|_| widgets_extra::split_pane(widgets::text("L"), widgets::text("R"), 0.3));
    assert_eq!(sem(&h).children.len(), 2);
    // First pane is narrower than the second (ratio 0.3).
    assert!(sem(&h).children[0].bounds.width() < sem(&h).children[1].bounds.width());
}

#[test]
fn text_area_accepts_multiline_input() {
    use lumen_core::events::TextInputEvent;
    let mut h = run(|cx| widgets_extra::text_area(cx, "ta", ""));
    assert_eq!(sem(&h).role, Role::TextInput);
    let p = mid(&sem(&h));
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.inject(Event::TextInput(TextInputEvent {
        text: "line1\nline2".into(),
    }));
    h.pump();
    assert_eq!(sem(&h).value.as_deref(), Some("line1\nline2"));
}
