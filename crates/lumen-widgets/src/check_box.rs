//! [`CheckBox`] — a self-stateful boolean toggle with a label. Its `Element` is
//! built inside [`CheckBox::new`]; the state lives in a signal keyed by `name`.

use crate::widget::impl_common;
use crate::{widgets, BuildCx, Element};
use lumen_core::semantics::{Action, Role, State as SemState};
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use std::rc::Rc;

const BOX: f64 = 20.0;

/// A checkbox: click (or Space when focused) toggles the boolean stored under
/// `name`. Checked shows a tick on a filled box; unchecked is an empty outline.
pub struct CheckBox {
    el: Element,
}

/// A white checkmark drawn to fill the box (shown when checked).
fn tick() -> Element {
    widgets::canvas(BOX, BOX, |f, size| {
        use kurbo::{BezPath, Point};
        let (w, h) = (size.width, size.height);
        let mut p = BezPath::new();
        p.move_to(Point::new(w * 0.26, h * 0.52));
        p.line_to(Point::new(w * 0.43, h * 0.70));
        p.line_to(Point::new(w * 0.76, h * 0.30));
        f.stroke(&p, Color::WHITE, 2.4);
    })
}

impl CheckBox {
    /// A checkbox labelled `label`, state stored under `name`.
    pub fn new(cx: &BuildCx, name: &str, label: impl Into<String>) -> CheckBox {
        let label = label.into();
        let checked = cx.signal(name, || false);
        let is = checked.get(cx.runtime());

        let boxel = Element {
            background: Some(if is {
                Color::srgb8(0x1a, 0x73, 0xe8, 0xff)
            } else {
                Color::srgb8(0xe6, 0xe9, 0xef, 0xff)
            }),
            corner_radius: 4.0,
            style: LayoutStyle {
                width: Dim::px(BOX as f32),
                height: Dim::px(BOX as f32),
                display: Display::Flex,
                align_items: Some(Align::Center),
                justify_content: Some(Align::Center),
                ..LayoutStyle::default()
            },
            children: if is { vec![tick()] } else { vec![] },
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
                align_items: Some(Align::Center),
                column_gap: Dim::px(8.0),
                ..LayoutStyle::default()
            },
            on_click: Some(Rc::new(move |rt| checked.update(rt, |c| *c = !*c))),
            children: vec![boxel, Element::text(label)],
            ..Element::default()
        };
        CheckBox { el }
    }

    /// Set the label text colour (e.g. to match a dark theme).
    pub fn color(mut self, c: Color) -> CheckBox {
        if let Some(ts) = self.el.children.last_mut().and_then(|e| e.text_style_mut()) {
            ts.color = c;
        }
        self
    }
}

impl_common!(CheckBox);
