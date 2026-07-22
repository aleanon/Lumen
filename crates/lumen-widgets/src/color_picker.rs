//! [`ColorPicker`] (W.2) — a swatch trigger opening a preset palette grid;
//! the chosen color's hex lands in the `{name}` signal (`String`, `#rrggbb`).
//! Arbitrary-color (wheel/eyedropper) selection is out of scope until a
//! native dialog arrives with P.4 — the palette covers themed-app needs.

use crate::widget::impl_common;
use crate::{widgets, BuildCx, Element};
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_layout::{Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use std::rc::Rc;

const PRESETS: [&str; 12] = [
    "#1a73e8", "#188a42", "#c98a0b", "#d32f2f", "#8e24aa", "#00838f", "#5d4037", "#455a64",
    "#e91e63", "#7cb342", "#f4511e", "#111418",
];

/// A palette color picker.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, ColorPicker, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     // Rendered with the palette open (see the `.open` signal below).
///     centered(cx, ColorPicker::new(cx, "brand").into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot_open(app, 240.0, 240.0, "color_picker", "brand.open");
/// ```
///
/// Renders:
///
/// ![Color Picker example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/color_picker.png)
///
/// The picture above is `src/doc_shots/color_picker.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct ColorPicker {
    el: Element,
}

impl ColorPicker {
    /// A picker storing the chosen hex under `name` (default `#1a73e8`).
    pub fn new(cx: &BuildCx, name: &str) -> ColorPicker {
        let value = cx.signal(name, || "#1a73e8".to_string());
        let open = cx.signal(&format!("{name}.open"), || false);
        let current = value.get(cx.runtime());
        let is_open = open.get(cx.runtime());

        // Trigger: the current color as a bordered swatch.
        let mut trigger = Element::default().class("swatch");
        trigger.role = Role::Button;
        trigger.label = format!("color {current}");
        trigger.focusable = true;
        trigger.background = Color::from_hex(&current).ok();
        trigger.border = Some(lumen_render::Border {
            width: 1.0,
            color: Color::srgb8(0xd8, 0xdd, 0xe3, 0xff),
        });
        trigger.corner_radius = 6.0;
        trigger.style.width = Dim::px(28.0);
        trigger.style.height = Dim::px(28.0);
        trigger.on_click = Some(Rc::new(move |rt| open.update(rt, |o| *o = !*o)));

        let mut children = vec![trigger];
        if is_open {
            let mut rows: Vec<Element> = Vec::new();
            for chunk in PRESETS.chunks(4) {
                let cells: Vec<Element> = chunk
                    .iter()
                    .map(|hex| {
                        let hex_s = hex.to_string();
                        let mut c = Element::default().class("cell");
                        c.role = Role::Button;
                        c.label = hex_s.clone();
                        c.focusable = true;
                        c.background = Color::from_hex(hex).ok();
                        c.corner_radius = 5.0;
                        c.style.width = Dim::px(24.0);
                        c.style.height = Dim::px(24.0);
                        c.on_click = Some(Rc::new(move |rt| {
                            value.set(rt, hex_s.clone());
                            open.set(rt, false);
                        }));
                        c
                    })
                    .collect();
                let mut r = widgets::row(cells);
                r.style.column_gap = Dim::px(6.0);
                rows.push(r);
            }
            let mut panel = widgets::column(rows);
            panel.background = Some(Color::srgb8(0xff, 0xff, 0xff, 0xff));
            panel.corner_radius = 8.0;
            panel.shadow = Some(crate::element::Shadow::soft());
            panel.overlay = true;
            panel.on_dismiss = Some(Rc::new(move |rt| open.set(rt, false)));
            panel.style.position = Position::Absolute;
            panel.style.inset = Edges {
                top: Dim::pct(1.0),
                left: Dim::px(0.0),
                ..Edges::AUTO
            };
            panel.style.margin.top = Dim::px(4.0);
            panel.style.row_gap = Dim::px(6.0);
            panel.style.padding = Edges::all(Dim::px(8.0));
            children.push(panel);
        }

        let el = Element {
            role: Role::Group,
            style: LayoutStyle {
                position: Position::Relative,
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..LayoutStyle::default()
            },
            children,
            ..Element::default()
        };
        ColorPicker { el }
    }
}

impl_common!(ColorPicker);
