//! [`Radio`] — one option in a single-choice group. Its `Element` is built inside
//! [`Radio::new`]; the selection lives in a signal keyed by the group name (the
//! shared `group` string), so radios with the same group are mutually exclusive.

use crate::widget::impl_common;
use crate::{BuildCx, Element};
use lumen_core::semantics::{Action, Role, State as SemState};
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use std::rc::Rc;

/// A radio button for `value` within group `group`. Selecting it sets the group
/// signal to `value`; it renders filled when the group equals `value`.
pub struct Radio {
    el: Element,
}

impl Radio {
    /// A radio labelled `label` selecting `value` in group `group`.
    pub fn new(
        cx: &BuildCx,
        group: &str,
        value: impl Into<String>,
        label: impl Into<String>,
    ) -> Radio {
        let value = value.into();
        let label = label.into();
        let selected = cx.signal(group, String::new);
        let cur = selected.get(cx.runtime());
        let is = cur == value;

        // Outer ring + (when selected) an inner dot.
        let ring_color = if is {
            Color::srgb8(0x1a, 0x73, 0xe8, 0xff)
        } else {
            Color::srgb8(0xbf, 0xc4, 0xcc, 0xff)
        };
        let mut ring = Element {
            background: Some(ring_color),
            corner_radius: 10.0,
            style: LayoutStyle {
                width: Dim::px(20.0),
                height: Dim::px(20.0),
                display: Display::Flex,
                align_items: Some(Align::Center),
                justify_content: Some(Align::Center),
                ..LayoutStyle::default()
            },
            ..Element::default()
        };
        if is {
            let dot = Element {
                background: Some(Color::WHITE),
                corner_radius: 4.0,
                style: LayoutStyle {
                    width: Dim::px(8.0),
                    height: Dim::px(8.0),
                    ..LayoutStyle::default()
                },
                ..Element::default()
            };
            ring.children = vec![dot];
        }

        let on_set = value.clone();
        let el = Element {
            role: Role::Radio,
            label: label.clone(),
            focusable: true,
            actions: vec![Action::Click, Action::Focus],
            states: vec![if is {
                SemState::Selected
            } else {
                SemState::Unchecked
            }],
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: Some(Align::Center),
                column_gap: Dim::px(8.0),
                ..LayoutStyle::default()
            },
            on_click: Some(Rc::new(move |rt| selected.set(rt, on_set.clone()))),
            children: vec![ring, Element::text(label)],
            ..Element::default()
        };
        Radio { el }
    }

    /// Set the label text colour (e.g. to match a dark theme).
    pub fn color(mut self, c: Color) -> Radio {
        if let Some(ts) = self.el.children.last_mut().and_then(|e| e.text_style_mut()) {
            ts.color = c;
        }
        self
    }
}

impl_common!(Radio);
