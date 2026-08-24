//! [`CheckBox`] — a self-stateful boolean toggle with a label. Its `Element` is
//! built inside [`CheckBox::new`]; the state lives in a signal keyed by `name`.

use crate::widget::{impl_widget, Common, Widget};
use crate::{widgets, BuildCx, Element};
use lumen_core::semantics::{Action, Role, State as SemState};
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use std::rc::Rc;

const BOX: f64 = 20.0;

/// A checkbox: click (or Space when focused) toggles the boolean stored under
/// `name`. Checked shows a tick on a filled box; unchecked is an empty outline.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, CheckBox, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, CheckBox::new(cx, "agree", "I agree").into())
/// }
/// # let app = App::new(build);
/// # // Rendered checked: doc_shot_open sets the `agree` boolean before shooting.
/// # lumen_widgets::doc_shot_open(app, 170.0, 52.0, "check_box", "agree");
/// ```
///
/// Renders:
///
/// ![Check Box example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/check_box.png)
///
/// The picture above is `src/doc_shots/check_box.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct CheckBox {
    label: crate::Text,
    /// The signal's *current* value, read at construction.
    ///
    /// The read has to stay eager — it is a tracked dependency, and moving it
    /// into `build` would move the dependency edge with it. What is deferred is
    /// the two-`Element` subtree the eager version allocated on the spot.
    checked: bool,
    /// The handle the click handler toggles. `Signal` is `Copy` (ADR-021), so
    /// keeping it is eight bytes, not a captured closure.
    signal: lumen_core::state::Signal<bool>,
    /// Label colour override.
    color: Option<Color>,
    common: Common,
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
    pub fn new(cx: &BuildCx, name: &str, label: impl Into<crate::Text>) -> CheckBox {
        let signal = cx.signal(name, || false);
        CheckBox {
            label: label.into(),
            checked: signal.get(cx.runtime()),
            signal,
            color: None,
            common: Common::default(),
        }
    }

    /// Set the label text colour (e.g. to match a dark theme).
    pub fn color(mut self, c: Color) -> CheckBox {
        self.color = Some(c);
        self
    }
}

impl Widget for CheckBox {
    fn build(self) -> Element {
        let CheckBox {
            label,
            checked,
            signal,
            color,
            common,
        } = self;

        let boxel = Element {
            background: Some(if checked {
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
            children: if checked { vec![tick()] } else { vec![] },
            ..Element::default()
        };

        let (label_s, label_dyn) = label.clone().into_parts();
        let mut text = Element::text(label);
        if let Some(c) = color {
            if let Some(ts) = text.text_style_mut() {
                ts.color = c;
            }
        }

        let mut el = Element {
            role: Role::Checkbox,
            label: label_s,
            dyn_text: label_dyn,
            focusable: true,
            actions: vec![Action::Click, Action::Focus],
            states: vec![if checked {
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
            on_click: Some(Rc::new(move |rt| signal.update(rt, |c| *c = !*c))),
            children: vec![boxel, text],
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(CheckBox);
