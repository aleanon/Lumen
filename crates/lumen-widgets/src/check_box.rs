//! [`CheckBox`] — a self-stateful boolean toggle with a label. Its `Element` is
//! built inside [`CheckBox::new`]; the state lives in a signal keyed by `name`.

use crate::widget::impl_common;
use crate::{BuildCx, Element};
use lumen_core::semantics::{Action, Role, State as SemState};
use lumen_core::Color;
use lumen_layout::{Dim, Display, FlexDirection, LayoutStyle};
use std::rc::Rc;

/// A checkbox: click (or Space when focused) toggles the boolean stored under
/// `name`.
pub struct CheckBox {
    el: Element,
}

impl CheckBox {
    /// A checkbox labelled `label`, state stored under `name`.
    pub fn new(cx: &BuildCx, name: &str, label: impl Into<String>) -> CheckBox {
        let label = label.into();
        let checked = cx.signal(name, || false);
        let is = checked.get(cx.runtime());
        let box_color = if is {
            Color::srgb8(0x1a, 0x73, 0xe8, 0xff)
        } else {
            Color::srgb8(0xcc, 0xcc, 0xcc, 0xff)
        };
        let boxel = Element {
            background: Some(box_color),
            corner_radius: 4.0,
            style: LayoutStyle {
                width: Dim::px(20.0),
                height: Dim::px(20.0),
                ..LayoutStyle::default()
            },
            ..Element::default()
        };
        let el = Element {
            role: Role::Checkbox,
            label: label.clone(),
            focusable: true,
            actions: vec![Action::Click, Action::Focus],
            states: vec![if is {
                SemState::Checked
            } else {
                SemState::Unchecked
            }],
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                column_gap: Dim::px(8.0),
                ..LayoutStyle::default()
            },
            on_click: Some(Rc::new(move |rt| checked.update(rt, |c| *c = !*c))),
            children: vec![boxel, Element::text(label)],
            ..Element::default()
        };
        CheckBox { el }
    }
}

impl_common!(CheckBox);
