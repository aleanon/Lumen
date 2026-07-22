//! [`Slider`] — a self-stateful horizontal slider over `[min, max]`. Its
//! `Element` (track + thumb + draggable container) is built inside
//! [`Slider::new`]; the value lives in a signal keyed by `name`.

use crate::widget::impl_common;
use crate::{BuildCx, Element};
use lumen_core::semantics::{Action, Role};
use lumen_core::Color;
use lumen_layout::{Dim, Edges, LayoutStyle, Position};
use std::rc::Rc;

const W: f64 = 200.0;
const THUMB: f64 = 16.0;

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
}

impl Slider {
    /// A slider over `[min, max]`, value stored under `name`.
    pub fn new(cx: &BuildCx, name: &str, min: f64, max: f64) -> Slider {
        let value = cx.signal(name, || min);
        let v = value.get(cx.runtime());
        let frac = ((v - min) / (max - min)).clamp(0.0, 1.0);

        let track = Element {
            background: Some(Color::srgb8(0xcc, 0xcc, 0xcc, 0xff)),
            corner_radius: 2.0,
            style: LayoutStyle {
                position: Position::Absolute,
                inset: Edges {
                    left: Dim::px(0.0),
                    top: Dim::px(8.0),
                    ..Edges::AUTO
                },
                width: Dim::px(W as f32),
                height: Dim::px(4.0),
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
            value: Some(format!("{v:.0}")),
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
            children: vec![track, thumb],
            ..Element::default()
        };
        Slider { el }
    }
}

impl_common!(Slider);
