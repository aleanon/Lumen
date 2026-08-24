//! [`Radio`] — one option in a single-choice group. Its `Element` is built inside
//! [`Radio::new`]; the selection lives in a signal keyed by the group name (the
//! shared `group` string), so radios with the same group are mutually exclusive.

use crate::widget::{impl_widget, Common, Widget};
use crate::{BuildCx, Element};
use lumen_core::semantics::{Action, Role, State as SemState};
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use std::rc::Rc;

/// A radio button for `value` within group `group`. Selecting it sets the group
/// signal to `value`; it renders filled when the group equals `value`.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Radio, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Radio::new(cx, "color", "red", "Red").into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 140.0, 52.0, "radio");
/// ```
///
/// Renders:
///
/// ![Radio example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/radio.png)
///
/// The picture above is `src/doc_shots/radio.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Radio {
    value: String,
    label: String,
    /// Whether this member is the selected one, resolved where the `BuildCx` is.
    selected: bool,
    signal: lumen_core::state::Signal<String>,
    color: Option<Color>,
    common: Common,
}

impl Radio {
    /// One option in the single-choice group `group`; picking it sets the
    /// group's signal to `value`.
    pub fn new(
        cx: &BuildCx,
        group: &str,
        value: impl Into<String>,
        label: impl Into<String>,
    ) -> Radio {
        let value = value.into();
        let signal = cx.signal(group, String::new);
        let selected = signal.get(cx.runtime()) == value;
        Radio {
            value,
            label: label.into(),
            selected,
            signal,
            color: None,
            common: Common::default(),
        }
    }

    /// Set the label text colour (e.g. to match a dark theme).
    pub fn color(mut self, c: Color) -> Radio {
        self.color = Some(c);
        self
    }
}

impl Widget for Radio {
    fn build(self) -> Element {
        let Radio {
            value,
            label,
            selected: is,
            signal,
            color,
            common,
        } = self;

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

        let mut text = Element::text(label.clone());
        if let Some(c) = color {
            if let Some(ts) = text.text_style_mut() {
                ts.color = c;
            }
        }

        let mut el = Element {
            role: Role::Radio,
            label,
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
            on_click: Some(Rc::new(move |rt| signal.set(rt, value.clone()))),
            children: vec![ring, text],
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Radio);
