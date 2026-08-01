//! Regression: a single-line editor that clips must scroll horizontally so the
//! caret stays inside its box.
//!
//! Lumen inputs had no caret-follow scroll: the caret's x is absolute, so once
//! the text is longer than the box, the caret sat past the right edge. A field
//! that clips (e.g. a wallet's masked password field) then hid the caret — you
//! couldn't see where typing happened. Focus was fine; only the caret was
//! clipped out of view.

use kurbo::{Point, Size};
use lumen_core::events::{Event, Key, KeyEvent, Modifiers, NamedKey, PointerEvent, TextInputEvent};
use lumen_layout::Dim;
use lumen_widgets::{App, BuildCx, Element, Headless, TextInput};

/// A narrow, **clipped** single-line input — the shape a masked field uses.
fn clipped_input() -> Headless {
    App::new(|cx: &mut BuildCx| {
        let mut el: Element = TextInput::new(cx, "in", "").into();
        el.clip = true;
        el.style.min_width = Dim::px(0.0);
        el.style.width = Dim::px(70.0);
        el.id("in")
    })
    .run_headless(Size::new(200.0, 80.0))
}

fn click_at(h: &mut Headless, p: Point) {
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
}

fn typ(h: &mut Headless, s: &str) {
    h.inject(Event::TextInput(TextInputEvent {
        text: s.to_string(),
    }));
    h.pump();
}

fn value(h: &Headless) -> String {
    h.semantics_doc().root.elided().value.unwrap_or_default()
}

fn key(h: &mut Headless, named: NamedKey) {
    h.inject(Event::KeyDown(KeyEvent {
        key: Key::Named(named),
        modifiers: Modifiers::empty(),
        repeat: false,
    }));
    h.pump();
}

#[test]
fn caret_follows_into_clipped_input() {
    let mut h = clipped_input();
    h.pump();
    let bx = h.node_bounds_by_id("in").expect("field laid out");

    // Focus by clicking, then type more than the 70px box can show.
    click_at(
        &mut h,
        Point::new((bx.x0 + bx.x1) / 2.0, (bx.y0 + bx.y1) / 2.0),
    );
    typ(&mut h, "0123456789012345678901234567890");

    // Focus is fine — text routed to the field (rules out a focus problem).
    assert!(!value(&h).is_empty(), "field is focused and receiving text");

    // The caret must stay inside the field's (clipped) box, not be pushed past
    // the right edge where the clip hides it.
    let caret = h.caret_rect("in").expect("focused editor paints a caret");
    assert!(
        caret.x1 <= bx.x1 + 0.5 && caret.x0 >= bx.x0 - 0.5,
        "caret {caret:?} escaped the clipped field box {bx:?} — no caret-follow scroll"
    );

    // Moving to the start scrolls back so the caret is visible at the left too
    // (the scroll follows the caret, it doesn't just clamp to the right edge).
    key(&mut h, NamedKey::Home);
    let caret0 = h.caret_rect("in").expect("caret at start still paints");
    assert!(
        caret0.x0 >= bx.x0 - 0.5 && caret0.x0 <= bx.x0 + 20.0,
        "caret {caret0:?} should return to the left of the box {bx:?} at Home"
    );
}
