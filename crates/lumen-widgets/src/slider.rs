//! [`Slider`] — a self-stateful horizontal slider over `[min, max]`. Its
//! `Element` (track + thumb + draggable container) is built inside
//! [`Slider::new`]; the value lives in a signal keyed by `name`.

use crate::widget::impl_common;
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
    el: Element,
    /// Kept so `.step()` can rebuild the handlers with the new step.
    cfg: (f64, f64, f64),
    value: lumen_core::Signal<f64>,
}

/// Format a value with enough decimals to be meaningful for its step.
///
/// The old fixed `{:.0}` made a `0.0..1.0` slider report `"0"` at every
/// position — the agent and assistive tech saw a control that never changed.
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
    /// A slider over `[min, max]`, value stored under `name`.
    ///
    /// Defaults to a continuous-feeling step of 1% of the range; override with
    /// [`Slider::step`].
    pub fn new(cx: &BuildCx, name: &str, min: f64, max: f64) -> Slider {
        let value = cx.signal(name, || min);
        let v = value.get(cx.runtime());
        let frac = ((v - min) / (max - min)).clamp(0.0, 1.0);
        let step = (max - min) / 100.0;

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

        let el = Element {
            role: Role::Slider,
            focusable: true,
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
        Slider {
            el,
            cfg: (min, max, step),
            value,
        }
    }

    /// Set the adjustment step used by the arrow keys, `Increment`/`Decrement`
    /// and `PageUp`/`PageDown` (which move ten steps).
    pub fn step(mut self, step: f64) -> Slider {
        let (min, max, _) = self.cfg;
        let step = step.abs().max(f64::EPSILON);
        self.cfg = (min, max, step);
        let value = self.value;
        // Re-derive everything that depends on the step.
        self.el.on_increment = Some(Rc::new(move |rt| {
            value.update(rt, |x| *x = (*x + step).clamp(min, max))
        }));
        self.el.on_decrement = Some(Rc::new(move |rt| {
            value.update(rt, |x| *x = (*x - step).clamp(min, max))
        }));
        self.el.on_key = Some(Rc::new(move |rt, ke| match ke.key {
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
        }));
        self
    }
}

impl_common!(Slider);
