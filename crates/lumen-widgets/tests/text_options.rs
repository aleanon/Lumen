//! W5 — the text-field options every framework has and Lumen lacked.
//!
//! `TextInput` had no placeholder, no length cap, no read-only mode, no
//! `on_change`, and **no password masking** — the only occurrence of "password"
//! in the crate was a comment. The Mercurium wallet had to hand-roll
//! `lumen_ui/src/widgets/masked_input.rs` because of it, which is the clearest
//! possible evidence the gap was real.
//!
//! The masking tests are the load-bearing ones: a password field that leaks its
//! value into the semantic tree leaks it to the agent, to assistive tech, and to
//! every `getTree` dump in a log.

use kurbo::{Point, Size};
use lumen_core::events::{Event, Key, KeyEvent, Modifiers, NamedKey, PointerEvent, TextInputEvent};
use lumen_core::semantics::{SemanticsNode, State as SemState};
use lumen_core::state::Signal;
use lumen_widgets::{App, BuildCx, Headless, TextInput};

fn sem(h: &Headless) -> SemanticsNode {
    h.semantics_doc().root.elided()
}

fn typ(h: &mut Headless, s: &str) {
    h.inject(Event::TextInput(TextInputEvent {
        text: s.to_string(),
    }));
    h.pump();
}

fn key(h: &mut Headless, named: NamedKey) {
    h.inject(Event::KeyDown(KeyEvent {
        key: Key::Named(named),
        modifiers: Modifiers::empty(),
        repeat: false,
    }));
    h.pump();
}

fn focus(h: &mut Headless) {
    let b = h.node_bounds_by_id("f").expect("field laid out");
    let p = Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
}

/// The stored (real) contents, via TextInput's published mirror.
fn stored(h: &Headless) -> String {
    TextInput::text_of(h.runtime(), "f")
}

#[test]
fn a_placeholder_shows_only_while_empty() {
    let mut h = App::new(|cx: &mut BuildCx| {
        TextInput::new(cx, "f", "")
            .placeholder("Your name")
            .id("f")
            .into()
    })
    .run_headless(Size::new(240.0, 80.0));
    h.pump();

    let node = sem(&h);
    assert_eq!(
        node.label, "Your name",
        "an empty field is labelled by its placeholder"
    );
    assert_eq!(node.value.as_deref(), Some(""), "but its value is empty");

    focus(&mut h);
    typ(&mut h, "Ada");
    assert_eq!(stored(&h), "Ada");
    let node = sem(&h);
    assert_eq!(node.value.as_deref(), Some("Ada"));
    assert_ne!(
        node.label, "Your name",
        "the placeholder is gone once typed"
    );
}

#[test]
fn a_password_field_masks_its_glyphs_and_semantics() {
    let mut h =
        App::new(|cx: &mut BuildCx| TextInput::new(cx, "f", "").password('•').id("f").into())
            .run_headless(Size::new(240.0, 80.0));
    h.pump();
    focus(&mut h);
    typ(&mut h, "hunter2");

    // The value is really stored...
    assert_eq!(stored(&h), "hunter2", "the field holds the real value");

    // ...but nothing the agent, AT or a screenshot can see reveals it.
    let node = sem(&h);
    assert_eq!(
        node.value.as_deref(),
        Some("•••••••"),
        "the published value is masked, one bullet per character"
    );
    assert!(
        !node.label.contains("hunter2"),
        "and the label never leaks it"
    );

    let dump = h.semantics_json().to_string();
    assert!(
        !dump.contains("hunter2"),
        "the secret must not appear anywhere in the semantic tree"
    );
}

/// A caret in a masked field indexes the *shown* glyphs. `•` is 3 bytes in
/// UTF-8, so a naive byte offset into the plaintext would put the caret in the
/// wrong place (or inside a glyph).
#[test]
fn a_masked_caret_tracks_the_bullets_not_the_plaintext() {
    let mut h =
        App::new(|cx: &mut BuildCx| TextInput::new(cx, "f", "").password('•').id("f").into())
            .run_headless(Size::new(240.0, 80.0));
    h.pump();
    focus(&mut h);
    typ(&mut h, "abcd");

    let caret = h.caret_rect("f").expect("focused field paints a caret");
    let box_ = h.node_bounds_by_id("f").unwrap();
    assert!(
        caret.x0 >= box_.x0 - 0.5 && caret.x1 <= box_.x1 + 0.5,
        "caret {caret:?} must stay inside the field {box_:?}"
    );

    // Moving left twice must move the caret visibly left.
    let at_end = caret.x0;
    key(&mut h, NamedKey::ArrowLeft);
    key(&mut h, NamedKey::ArrowLeft);
    let moved = h.caret_rect("f").expect("caret still painted").x0;
    assert!(
        moved < at_end,
        "the caret moved left across bullets ({moved} !< {at_end})"
    );
}

#[test]
fn max_length_caps_the_contents() {
    let mut h =
        App::new(|cx: &mut BuildCx| TextInput::new(cx, "f", "").max_length(5).id("f").into())
            .run_headless(Size::new(240.0, 80.0));
    h.pump();
    focus(&mut h);

    typ(&mut h, "abc");
    assert_eq!(stored(&h), "abc");
    // A burst that would overflow is truncated to what fits, not rejected whole.
    typ(&mut h, "defgh");
    assert_eq!(stored(&h), "abcde", "capped at five characters");
    typ(&mut h, "z");
    assert_eq!(stored(&h), "abcde", "and stays capped");
}

#[test]
fn a_read_only_field_cannot_be_edited_but_stays_readable() {
    let mut h = App::new(|cx: &mut BuildCx| {
        TextInput::new(cx, "f", "fixed")
            .read_only(true)
            .id("f")
            .into()
    })
    .run_headless(Size::new(240.0, 80.0));
    h.pump();
    focus(&mut h);

    typ(&mut h, "nope");
    assert_eq!(
        stored(&h),
        "fixed",
        "typing does not change a read-only field"
    );
    key(&mut h, NamedKey::Backspace);
    assert_eq!(stored(&h), "fixed", "nor does Backspace");

    let node = sem(&h);
    assert!(
        node.states.contains(&SemState::Readonly),
        "and it says so in semantics"
    );
    assert_eq!(
        node.value.as_deref(),
        Some("fixed"),
        "the content is still published — read-only, not hidden"
    );
}

#[test]
fn on_change_reports_every_edit() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let seen = cx.signal("seen", String::new);
        TextInput::new(cx, "f", "")
            .on_change(move |rt, v| seen.set(rt, v.to_string()))
            .id("f")
            .into()
    })
    .run_headless(Size::new(240.0, 80.0));
    h.pump();
    focus(&mut h);

    let seen: Signal<String> = h.runtime().signal("seen", String::new);
    typ(&mut h, "a");
    assert_eq!(seen.get(h.runtime()), "a", "on_change sees the full value");
    typ(&mut h, "b");
    assert_eq!(seen.get(h.runtime()), "ab", "not just the delta");
}

/// The options compose — a capped password field with a placeholder is exactly
/// the shape a wallet's PIN entry needs.
#[test]
fn the_options_compose() {
    let mut h = App::new(|cx: &mut BuildCx| {
        TextInput::new(cx, "f", "")
            .password('*')
            .max_length(4)
            .placeholder("PIN")
            .id("f")
            .into()
    })
    .run_headless(Size::new(240.0, 80.0));
    h.pump();
    assert_eq!(sem(&h).label, "PIN", "empty shows the placeholder");

    focus(&mut h);
    typ(&mut h, "123456");
    assert_eq!(stored(&h), "1234", "capped");
    assert_eq!(
        sem(&h).value.as_deref(),
        Some("****"),
        "masked with the chosen bullet"
    );
    h.assert_view_coherent();
}
