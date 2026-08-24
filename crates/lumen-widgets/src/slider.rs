//! [`Slider`] — a self-stateful horizontal slider over `[min, max]`. Its
//! `Element` (track + thumb + draggable container) is built inside
//! [`Slider::new`]; the value lives in a signal keyed by `name`.

use crate::widget::{impl_widget, Common, Widget};
use crate::{BuildCx, Element};
use lumen_core::events::{Key, NamedKey};
use lumen_core::semantics::{Action, Role};
use lumen_core::Color;
use lumen_layout::{Dim, Edges, LayoutStyle, Position};
use std::rc::Rc;

const W: f64 = 200.0;
const THUMB: f64 = 16.0;
/// Track thickness.
const TRACK_H: f64 = 4.0;
/// Track top, so the bar is centred on the thumb's axis.
const TRACK_TOP: f64 = (THUMB - TRACK_H) / 2.0;

/// A horizontal slider; drag or press to set the value from the pointer position.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{full_width, Slider, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     full_width(cx, Slider::new(cx, "vol", 0.0, 100.0).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 240.0, 56.0, "slider");
/// ```
///
/// Renders:
///
/// ![Slider example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/slider.png)
///
/// The picture above is `src/doc_shots/slider.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Slider {
    min: f64,
    max: f64,
    /// An explicit step, or `None` for the default hundredth of the range.
    ///
    /// Storing the choice is the whole saving here: the eager `.step()` had to
    /// **rebuild three `Rc` closures** that `::new()` had just allocated, because
    /// the increment, decrement and key handlers all close over the step.
    step: Option<f64>,
    /// The signal's current value, read where the `BuildCx` is.
    current: f64,
    value: lumen_core::Signal<f64>,
    common: Common,
}

/// Format `v` with as many decimals as `step` warrants.
fn fmt_value(v: f64, step: f64) -> String {
    if step >= 1.0 {
        format!("{v:.0}")
    } else if step >= 0.1 {
        format!("{v:.1}")
    } else if step >= 0.01 {
        format!("{v:.2}")
    } else {
        format!("{v:.3}")
    }
}

impl Slider {
    /// A horizontal slider over `min..=max`; the value lives in the signal
    /// keyed by `name`.
    pub fn new(cx: &BuildCx, name: &str, min: f64, max: f64) -> Slider {
        let value = cx.signal(name, || min);
        Slider {
            min,
            max,
            step: None,
            current: value.get(cx.runtime()),
            value,
            common: Common::default(),
        }
    }

    /// Set the increment for arrows, `Action::Increment`, and value formatting.
    pub fn step(mut self, step: f64) -> Slider {
        self.step = Some(step.abs().max(f64::EPSILON));
        self
    }
}

impl Widget for Slider {
    fn build(self) -> Element {
        let Slider {
            min,
            max,
            step,
            current: v,
            value,
            common,
        } = self;
        let step = step.unwrap_or((max - min) / 100.0);
        let frac = ((v - min) / (max - min)).clamp(0.0, 1.0);

        let track = Element {
            background: Some(Color::srgb8(0xcc, 0xcc, 0xcc, 0xff)),
            corner_radius: 2.0,
            style: LayoutStyle {
                position: Position::Absolute,
                inset: Edges {
                    left: Dim::px(0.0),
                    top: Dim::px(TRACK_TOP as f32),
                    ..Edges::AUTO
                },
                width: Dim::px(W as f32),
                height: Dim::px(TRACK_H as f32),
                ..LayoutStyle::default()
            },
            ..Element::default()
        }
        .part("track");
        // Centre the thumb on the value's position along the *full* track so it
        // sits directly under the pointer while dragging (clamped to the ends),
        // rather than lagging behind (which `frac * (W - THUMB)` would do).
        let thumb_left = (frac * W - THUMB / 2.0).clamp(0.0, W - THUMB);
        let thumb = Element {
            background: Some(Color::srgb8(0x1a, 0x73, 0xe8, 0xff)),
            corner_radius: THUMB / 2.0,
            style: LayoutStyle {
                position: Position::Absolute,
                inset: Edges {
                    left: Dim::px(thumb_left as f32),
                    top: Dim::px(0.0),
                    ..Edges::AUTO
                },
                width: Dim::px(THUMB as f32),
                height: Dim::px(THUMB as f32),
                ..LayoutStyle::default()
            },
            ..Element::default()
        }
        .part("thumb");

        let mut el = Element {
            role: Role::Slider,
            focusable: true,
            // Formatted with the *final* step. The eager version formatted in
            // `::new()` with the default one, so `.step(0.01)` left the
            // accessible value rounded to whole numbers.
            value: Some(fmt_value(v, step)),
            actions: vec![Action::SetValue, Action::Increment, Action::Decrement],
            style: LayoutStyle {
                position: Position::Relative,
                width: Dim::px(W as f32),
                height: Dim::px(THUMB as f32),
                ..LayoutStyle::default()
            },
            // Horizontal control → the x fraction along the track sets the value.
            on_drag: Some(Rc::new(move |rt, fx, _fy, _pos| {
                value.set(rt, min + fx * (max - min))
            })),
            // W2: the declared actions are implemented, so `input.invokeAction`
            // and a screen reader can drive the slider without pixel geometry.
            on_increment: Some(Rc::new(move |rt| {
                value.update(rt, |x| *x = (*x + step).clamp(min, max))
            })),
            on_decrement: Some(Rc::new(move |rt| {
                value.update(rt, |x| *x = (*x - step).clamp(min, max))
            })),
            on_set_value: Some(Rc::new(move |rt, s| {
                if let Ok(n) = s.parse::<f64>() {
                    value.set(rt, n.clamp(min, max));
                }
            })),
            // W3: the WAI-ARIA slider keys.
            on_key: Some(Rc::new(move |rt, ke| match ke.key {
                Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::ArrowUp) => {
                    value.update(rt, |x| *x = (*x + step).clamp(min, max))
                }
                Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowDown) => {
                    value.update(rt, |x| *x = (*x - step).clamp(min, max))
                }
                Key::Named(NamedKey::Home) => value.set(rt, min),
                Key::Named(NamedKey::End) => value.set(rt, max),
                Key::Named(NamedKey::PageUp) => {
                    value.update(rt, |x| *x = (*x + step * 10.0).clamp(min, max))
                }
                Key::Named(NamedKey::PageDown) => {
                    value.update(rt, |x| *x = (*x - step * 10.0).clamp(min, max))
                }
                _ => {}
            })),
            children: vec![track, thumb],
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Slider);
