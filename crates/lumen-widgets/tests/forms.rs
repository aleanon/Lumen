//! T5.5 acceptance: validators, and an agent reading validation failures as
//! structured data (State::Invalid + error node), fixing them, and submitting.

use kurbo::{Point, Size};
use lumen_core::events::{Event, PointerEvent, TextInputEvent};
use lumen_core::semantics::{SemanticsNode, State};
use lumen_widgets::forms::{form_field, validate, Validator};
use lumen_widgets::{widgets, App, BuildCx, Element, Headless};

#[test]
fn validators_check_values() {
    assert_eq!(
        validate("", &[Validator::Required]).as_deref(),
        Some("required")
    );
    assert_eq!(validate("ok", &[Validator::Required]), None);
    assert!(validate("a", &[Validator::MinLen(3)]).is_some());
    assert_eq!(validate("ada@x.com", &[Validator::Email]), None);
    assert!(validate("nope", &[Validator::Email]).is_some());
    assert!(
        validate("a@b", &[Validator::Email]).is_some(),
        "needs a dot in domain"
    );
    // First failing validator wins.
    assert_eq!(
        validate("", &[Validator::Required, Validator::Email]).as_deref(),
        Some("required")
    );
}

fn form(cx: &mut BuildCx) -> Element {
    widgets::column(vec![
        form_field(
            cx,
            "email",
            "Email",
            vec![Validator::Required, Validator::Email],
        ),
        widgets::text("Sign up").id("title"),
    ])
}

fn node(h: &Headless, id: &str) -> Option<SemanticsNode> {
    fn find(n: &SemanticsNode, id: &str) -> Option<SemanticsNode> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.clone());
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    let _ = h;
    find(&h.semantics_doc().root.elided(), id)
}

fn type_into(h: &mut Headless, input_id: &str, text: &str) {
    let b = node(h, input_id).unwrap().bounds;
    let p = Point::new(b.x0 + b.width() / 2.0, b.y0 + b.height() / 2.0);
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.inject(Event::TextInput(TextInputEvent { text: text.into() }));
    h.pump();
}

#[test]
fn agent_reads_and_fixes_validation_errors() {
    let mut h = App::new(form).run_headless(Size::new(300.0, 200.0));

    // Initially empty → invalid (required) → structured error visible.
    assert!(node(&h, "email").unwrap().states.contains(&State::Invalid));
    assert_eq!(node(&h, "email-error").unwrap().label, "required");

    // Type an invalid email → error changes to "invalid email".
    type_into(&mut h, "email-input", "nope");
    assert!(node(&h, "email").unwrap().states.contains(&State::Invalid));
    assert_eq!(node(&h, "email-error").unwrap().label, "invalid email");

    // Fix it → field becomes valid and the error node disappears.
    type_into(&mut h, "email-input", "@lumen.dev");
    assert_eq!(
        node(&h, "email-input").unwrap().value.as_deref(),
        Some("nope@lumen.dev")
    );
    assert!(
        !node(&h, "email").unwrap().states.contains(&State::Invalid),
        "now valid"
    );
    assert!(node(&h, "email-error").is_none(), "error cleared");
}
