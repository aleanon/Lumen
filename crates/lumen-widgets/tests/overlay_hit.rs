//! Overlays (dropdown menus, popovers) paint in a final top pass that escapes
//! ancestor clips. Hit-testing must agree: a click where the overlay paints
//! goes to the overlay, not to the normal-flow content it covers — even though
//! that content comes later in document order. (Regression: without an elevated
//! hit-test z, the widget *under* the open menu stole the option clicks.)

use kurbo::{Point, Rect, Size};
use lumen_core::events::{Event, PointerEvent};
use lumen_core::semantics::{Role, SemanticsNode};
use lumen_widgets::{App, BuildCx, Button, Headless, PickList};
use std::cell::Cell;
use std::rc::Rc;

fn by_id<'a>(n: &'a SemanticsNode, id: &str) -> Option<&'a SemanticsNode> {
    if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
        return Some(n);
    }
    n.children.iter().find_map(|c| by_id(c, id))
}

fn bounds(h: &Headless, id: &str) -> Rect {
    by_id(&h.semantics_doc().root.elided(), id)
        .unwrap_or_else(|| panic!("no node {id}"))
        .bounds
}

fn click(h: &mut Headless, p: Point) {
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
}

fn text_count(n: &SemanticsNode) -> usize {
    let mut c = usize::from(matches!(n.role, Role::Text));
    for ch in &n.children {
        c += text_count(ch);
    }
    c
}

#[test]
fn open_dropdown_option_wins_over_widget_beneath_it() {
    let btn_hits = Rc::new(Cell::new(0));
    let bh = btn_hits.clone();
    let mut h = App::new(move |cx: &mut BuildCx| {
        let c = bh.clone();
        lumen_widgets::widgets::column(vec![
            PickList::new(cx, "pl", "Pick", ["Apple", "Banana", "Cherry"])
                .id("pl")
                .into(),
            // A button placed right below the trigger, so the open menu paints
            // over it.
            Button::new("Behind")
                .on_press(move |_| c.set(c.get() + 1))
                .id("behind")
                .into(),
        ])
    })
    .run_headless(Size::new(360.0, 320.0));

    // Open the menu by clicking the trigger.
    let pl = bounds(&h, "pl");
    click(&mut h, Point::new((pl.x0 + pl.x1) / 2.0, pl.y0 + 19.0));
    let open_texts = text_count(&h.semantics_doc().root.elided());
    assert!(open_texts >= 4, "menu should be open (trigger + 3 options)");

    // The "Behind" button overlaps where the first option paints.
    let behind = bounds(&h, "behind");
    let first_opt_y = behind.y0 + 21.0; // inside both the option row and the button
    assert!(
        behind.contains(Point::new(behind.x0 + 5.0, first_opt_y)),
        "probe point is within the button's box (it's beneath the menu)"
    );

    // Click the first option where it overlaps the button.
    click(&mut h, Point::new(behind.x0 + 20.0, first_opt_y));

    // The option was chosen (menu closed) and the button did NOT fire.
    assert!(
        text_count(&h.semantics_doc().root.elided()) < open_texts,
        "clicking the option should select it and close the menu"
    );
    assert_eq!(
        btn_hits.get(),
        0,
        "the widget beneath the overlay must not steal the click"
    );
}
