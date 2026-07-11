//! [`RangeSlider`] (W.2) — two thumbs bounding a sub-range of `[min, max]`.
//! Values live in `{name}.lo` / `{name}.hi` signals; dragging moves whichever
//! thumb is nearer the pointer, and the thumbs cannot cross.

use crate::widget::impl_common;
use crate::Element;
use lumen_core::semantics::{Action, Role};
use lumen_core::Color;
use lumen_layout::{Dim, Edges, LayoutStyle, Position};
use std::rc::Rc;

const W: f64 = 200.0;
const THUMB: f64 = 16.0;

/// A double-ended slider over `[min, max]`.
pub struct RangeSlider {
    el: Element,
}

impl RangeSlider {
    /// A range slider storing its ends under `{name}.lo` / `{name}.hi`.
    pub fn new(cx: &crate::BuildCx, name: &str, min: f64, max: f64) -> RangeSlider {
        let lo = cx.signal(&format!("{name}.lo"), || min);
        let hi = cx.signal(&format!("{name}.hi"), || max);
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
                    top: Dim::px(8.0),
                    ..Edges::AUTO
                },
                width: Dim::px((fill_x1 - fill_x0).max(0.0) as f32),
                height: Dim::px(4.0),
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
            actions: vec![Action::SetValue],
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
        RangeSlider { el }
    }
}

impl_common!(RangeSlider);
