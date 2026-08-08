//! MOD4: a third party can add a `.lss` property without forking.
//!
//! `Style::apply` is a closed `match`, so before this the answer to "can I add
//! a style property?" was "fork the crate" — the one clearly-absent extension
//! point against the architecture doc's claim that third-party widgets and
//! styling are first-class.
//!
//! Registration is process-global, so these tests use distinct property names
//! rather than assuming a clean registry per test.

use lumen_style::{apply, has_errors, parse, register_property, Style, Tokens};

fn parse_one(prop: &str, value: &str) -> Style {
    let (sheet, diags) = parse("t.lss", &format!("card {{ {prop}: {value}; }}"));
    assert!(
        !has_errors(&diags),
        "a registered property must parse cleanly, got: {diags:?}"
    );
    let mut s = Style::new();
    // Pull the declaration back out of the AST and apply it.
    for item in &sheet.items {
        if let lumen_style::Item::Rule(r) = item {
            for d in &r.declarations {
                apply(&mut s, &d.property, &d.value, &Tokens::new());
            }
        }
    }
    s
}

#[test]
fn a_registered_property_parses_and_carries_its_value() {
    assert!(register_property("elevation"));
    let s = parse_one("elevation", "3");
    assert!(
        s.custom.contains_key("elevation"),
        "the resolved value must reach Style::custom, got {:?}",
        s.custom
    );
}

#[test]
fn an_unregistered_property_is_still_an_error() {
    // The registry must not weaken E0102 generally — a typo has to stay a typo,
    // or the diagnostic that catches misspelled properties becomes useless.
    let (_, diags) = parse("t.lss", "card { widht: 3px; }");
    assert!(
        has_errors(&diags),
        "an unknown property must still be E0102"
    );
}

#[test]
fn registering_over_a_builtin_is_refused() {
    // Silently shadowing `width` would be a far worse surprise than a rejected
    // call: layout would keep reading the built-in field while the extension
    // believed it owned the name.
    assert!(
        !register_property("width"),
        "registration must refuse to shadow a built-in property"
    );
}

#[test]
fn registration_is_idempotent() {
    assert!(register_property("corner-flourish"));
    assert!(
        register_property("corner-flourish"),
        "re-registering the same name must succeed, not fail"
    );
    assert!(lumen_style::is_registered("corner-flourish"));
}

#[test]
fn built_in_properties_are_unaffected() {
    // The extension path must not divert a real property into `custom`.
    let s = parse_one("width", "10px");
    assert!(s.width.is_some(), "a built-in must still set its own field");
    assert!(
        s.custom.is_empty(),
        "a built-in must not leak into custom: {:?}",
        s.custom
    );
}
