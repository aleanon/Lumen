//! [`RangeSlider`] (W.2) — two thumbs bounding a sub-range of `[min, max]`.
//! Values live in `{name}.lo` / `{name}.hi` signals; dragging moves whichever
//! thumb is nearer the pointer, and the thumbs cannot cross.

use crate::widget::impl_common;
use crate::Element;
use lumen_core::events::{Key, NamedKey};
use lumen_core::semantics::{Action, Role};
use lumen_core::Color;
use lumen_layout::{Dim, Edges, LayoutStyle, Position};
use std::rc::Rc;

const W: f64 = 200.0;
const THUMB: f64 = 16.0;
/// Track thickness.
const TRACK_H: f64 = 4.0;
/// Track top, so the bar is centred on the thumb's axis rather than sitting
/// below it — `THUMB / 2` is the centre line, and the bar straddles it.
const TRACK_TOP: f64 = (THUMB - TRACK_H) / 2.0;

/// A double-ended slider over `[min, max]`.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{full_width, RangeSlider, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     full_width(cx, RangeSlider::new(cx, "range", 0.0, 100.0).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 240.0, 56.0, "range_slider");
/// ```
///
/// Renders:
///
/// ![Range Slider example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/range_slider.png)
///
/// The picture above is `src/doc_shots/range_slider.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct RangeSlider {
    el: Element,
}

impl RangeSlider {
    /// A range slider storing its ends under `{name}.lo` / `{name}.hi`.
    pub fn new(cx: &crate::BuildCx, name: &str, min: f64, max: f64) -> RangeSlider {
        let lo = cx.signal(format!("{name}.lo"), || min);
        let hi = cx.signal(format!("{name}.hi"), || max);
        let (lo_v, hi_v) = (lo.get(cx.runtime()), hi.get(cx.runtime()));
        let span = (max - min).max(f64::EPSILON);
        let frac = |v: f64| ((v - min) / span).clamp(0.0, 1.0);

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

        // Filled band between the thumbs.
        let fill_x0 = frac(lo_v) * W;
        let fill_x1 = frac(hi_v) * W;
        let fill = Element {
            background: Some(crate::theme::accent()),
            corner_radius: 2.0,
            style: LayoutStyle {
                position: Position::Absolute,
                inset: Edges {
                    left: Dim::px(fill_x0 as f32),
                    top: Dim::px(TRACK_TOP as f32),
                    ..Edges::AUTO
                },
                width: Dim::px((fill_x1 - fill_x0).max(0.0) as f32),
                height: Dim::px(TRACK_H as f32),
                ..LayoutStyle::default()
            },
            ..Element::default()
        }
        .part("fill");

        let thumb = |v: f64, part: &str| {
            Element {
                background: Some(Color::srgb8(0x1a, 0x73, 0xe8, 0xff)),
                corner_radius: THUMB / 2.0,
                style: LayoutStyle {
                    position: Position::Absolute,
                    inset: Edges {
                        left: Dim::px((frac(v) * W - THUMB / 2.0).clamp(0.0, W - THUMB) as f32),
                        top: Dim::px(0.0),
                        ..Edges::AUTO
                    },
                    width: Dim::px(THUMB as f32),
                    height: Dim::px(THUMB as f32),
                    ..LayoutStyle::default()
                },
                ..Element::default()
            }
            .part(part)
        };

        let mut el = Element {
            role: Role::Slider,
            focusable: true,
            value: Some(format!("{lo_v:.0}–{hi_v:.0}")),
            actions: vec![Action::SetValue, Action::Increment, Action::Decrement],
            children: vec![
                track,
                fill,
                thumb(lo_v, "thumb-lo"),
                thumb(hi_v, "thumb-hi"),
            ],
            ..Element::default()
        };
        el.style.position = Position::Relative;
        el.style.width = Dim::px(W as f32);
        el.style.height = Dim::px(THUMB as f32);
        // Drag: the x fraction sets whichever end is nearer (ties → the one
        // the pointer is beyond); ends clamp so they can't cross.
        el.on_drag = Some(Rc::new(move |rt, fx, _fy, _pos| {
            let v = min + fx.clamp(0.0, 1.0) * span;
            let (l, h) = (lo.get(rt), hi.get(rt));
            if (v - l).abs() <= (v - h).abs() {
                lo.set(rt, v.min(h));
            } else {
                hi.set(rt, v.max(l));
            }
        }));
        // W2/W3: implement the actions this declares, and give the range the
        // keyboard. Plain arrows move the *upper* end (the common adjustment);
        // Shift+arrows move the lower one. Ends clamp so they cannot cross.
        let step = span / 100.0;
        let bump = move |rt: &lumen_core::state::Runtime, delta: f64, lower: bool| {
            let (l, h) = (lo.get(rt), hi.get(rt));
            if lower {
                lo.set(rt, (l + delta).clamp(min, h));
            } else {
                hi.set(rt, (h + delta).clamp(l, max));
            }
        };
        el.on_increment = Some(Rc::new(move |rt| bump(rt, step, false)));
        el.on_decrement = Some(Rc::new(move |rt| bump(rt, -step, false)));
        el.on_set_value = Some(Rc::new(move |rt, v| {
            // "lo,hi" sets both ends; a bare number moves the nearer end.
            if let Some((a, b)) = v.split_once(',') {
                if let (Ok(a), Ok(b)) = (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
                    let (a, b) = if a <= b { (a, b) } else { (b, a) };
                    lo.set(rt, a.clamp(min, max));
                    hi.set(rt, b.clamp(a.clamp(min, max), max));
                }
            } else if let Ok(n) = v.trim().parse::<f64>() {
                let (l, h) = (lo.get(rt), hi.get(rt));
                if (n - l).abs() <= (n - h).abs() {
                    lo.set(rt, n.clamp(min, h));
                } else {
                    hi.set(rt, n.clamp(l, max));
                }
            }
        }));
        el.on_key = Some(Rc::new(move |rt, ke| {
            let lower = ke.modifiers.contains(lumen_core::events::Modifiers::SHIFT);
            match ke.key {
                Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::ArrowUp) => {
                    bump(rt, step, lower)
                }
                Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowDown) => {
                    bump(rt, -step, lower)
                }
                Key::Named(NamedKey::Home) if lower => lo.set(rt, min),
                Key::Named(NamedKey::End) if !lower => hi.set(rt, max),
                _ => {}
            }
        }));
        RangeSlider { el }
    }
}

impl_common!(RangeSlider);
