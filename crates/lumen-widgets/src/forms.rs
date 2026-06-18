//! Forms & validation (T5.5): declarative field validators whose failures are
//! **structured data** — a field's error shows in the semantic tree as
//! `State::Invalid` plus an associated error node, so an agent reads and fixes
//! validation failures without looking at pixels.

use crate::element::{BuildCx, Element};
use crate::widgets;
use lumen_core::semantics::{Action, Role, State as SemState};
use lumen_core::Color;
use lumen_layout::{Dim, Display, Edges, FlexDirection, LayoutStyle};
use lumen_text::TextStyle;
use std::rc::Rc;

/// A field validator. Returns `Err(message)` on failure.
#[derive(Clone)]
pub enum Validator {
    /// Non-empty (after trimming).
    Required,
    /// At least `n` characters.
    MinLen(usize),
    /// At most `n` characters.
    MaxLen(usize),
    /// A plausible email address.
    Email,
    /// A custom predicate with a message.
    Custom(Rc<dyn Fn(&str) -> bool>, String),
}

impl Validator {
    /// Check `value`; `Ok(())` if it passes.
    pub fn check(&self, value: &str) -> Result<(), String> {
        match self {
            Validator::Required => {
                if value.trim().is_empty() {
                    Err("required".into())
                } else {
                    Ok(())
                }
            }
            Validator::MinLen(n) => {
                if value.chars().count() < *n {
                    Err(format!("must be at least {n} characters"))
                } else {
                    Ok(())
                }
            }
            Validator::MaxLen(n) => {
                if value.chars().count() > *n {
                    Err(format!("must be at most {n} characters"))
                } else {
                    Ok(())
                }
            }
            Validator::Email => {
                let ok = value.contains('@')
                    && value.split_once('@').is_some_and(|(u, d)| {
                        !u.is_empty() && d.contains('.') && !d.starts_with('.') && !d.ends_with('.')
                    });
                if ok {
                    Ok(())
                } else {
                    Err("invalid email".into())
                }
            }
            Validator::Custom(f, msg) => {
                if f(value) {
                    Ok(())
                } else {
                    Err(msg.clone())
                }
            }
        }
    }
}

/// Validate `value` against `validators`, returning the first error.
pub fn validate(value: &str, validators: &[Validator]) -> Option<String> {
    validators.iter().find_map(|v| v.check(value).err())
}

/// A labelled, validated text field. `name` keys the value signal; the input is
/// `#<name>-input`, the error (when invalid) is `#<name>-error`, and the field
/// carries `State::Invalid` while it fails validation.
pub fn form_field(cx: &BuildCx, name: &str, label: &str, validators: Vec<Validator>) -> Element {
    let value = cx.signal(name, String::new);
    let v = value.get(cx.runtime());
    let error = validate(&v, &validators);
    let invalid = error.is_some();

    let shown = if v.is_empty() {
        " ".to_string()
    } else {
        v.clone()
    };
    let input = Element {
        role: Role::TextInput,
        focusable: true,
        label: v.clone(),
        value: Some(v.clone()),
        actions: vec![Action::Focus, Action::SetValue],
        states: if invalid {
            vec![SemState::Invalid]
        } else {
            vec![]
        },
        background: Some(if invalid {
            Color::srgb8(0xfd, 0xec, 0xea, 0xff)
        } else {
            Color::srgb8(0xf2, 0xf2, 0xf2, 0xff)
        }),
        corner_radius: 4.0,
        style: LayoutStyle {
            padding: Edges::all(Dim::px(6.0)),
            min_width: Dim::px(180.0),
            ..LayoutStyle::default()
        },
        text: Some((shown, TextStyle::default())),
        on_text: Some(Rc::new(move |rt, t| {
            let t = t.to_string();
            value.update(rt, |s| s.push_str(&t))
        })),
        ..Element::default()
    }
    .id(format!("{name}-input"));

    let mut children = vec![widgets::text(label.to_string()), input];
    if let Some(e) = &error {
        children.push(
            Element {
                role: Role::Text,
                label: e.clone(),
                text: Some((
                    e.clone(),
                    TextStyle {
                        font_size: 12.0,
                        weight: 400.0,
                        color: Color::srgb8(0xc0, 0x39, 0x2b, 0xff),
                        line_height: None,
                        letter_spacing: 0.0,
                    },
                )),
                ..Element::default()
            }
            .id(format!("{name}-error")),
        );
    }

    Element {
        role: Role::Group,
        label: label.to_string(),
        states: if invalid {
            vec![SemState::Invalid]
        } else {
            vec![]
        },
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            row_gap: Dim::px(2.0),
            ..LayoutStyle::default()
        },
        children,
        ..Element::default()
    }
    .id(name)
}
