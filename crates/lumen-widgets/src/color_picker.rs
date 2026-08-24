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

/// Starting points, not the whole palette — the plane and bars reach every
/// colour, and clicking a preset seeds them rather than ending the choice.
/// Two rows of eight: a hue sweep, then greys and skin-ish neutrals.
const PRESETS: [&str; 16] = [
    "#d32f2fff",
    "#f4511eff",
    "#f9a825ff",
    "#7cb342ff",
    "#188a42ff",
    "#00838fff",
    "#1a73e8ff",
    "#8e24aaff",
    "#e91e63ff",
    "#5d4037ff",
    "#455a64ff",
    "#9aa4bbff",
    "#cfd4daff",
    "#ffffffff",
    "#6b7488ff",
    "#111418ff",
];

/// A palette color picker.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{widgets, ColorPicker, BuildCx, Element};
/// use lumen_layout::{Dim, Edges};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     // Rendered with the panel open (see the `.open` signal below). Placed
///     // top-left rather than centred: the panel is anchored to the trigger's
///     // left edge, so centring the trigger pushes the panel off the frame.
///     let mut col = widgets::column(vec![ColorPicker::new(cx, "brand").into()]);
///     col.style.padding = Edges::all(Dim::px(10.0));
///     col
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot_open(app, 265.0, 400.0, "color_picker", "brand.open");
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

/// Panel geometry.
const PLANE_W: f64 = 220.0;
const PLANE_H: f64 = 150.0;
const BAR_H: f64 = 14.0;

/// HSV (all in `0..=1`, hue turning once) → linear-light RGB.
fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    let h6 = (h.rem_euclid(1.0)) * 6.0;
    let i = h6.floor() as i32 % 6;
    let f = h6 - h6.floor();
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// RGB (`0..=1` sRGB) → HSV, so an incoming hex seeds the controls.
fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= f64::EPSILON {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h, if max <= 0.0 { 0.0 } else { d / max }, max)
}

/// An sRGB-byte colour from HSV + alpha.
fn hsva_color(h: f64, s: f64, v: f64, a: f64) -> Color {
    let (r, g, b) = hsv_to_rgb(h, s, v);
    Color::srgb8(
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
        (a * 255.0).round() as u8,
    )
}

/// `#rrggbbaa` for HSVA.
fn hsva_hex(h: f64, s: f64, v: f64, a: f64) -> String {
    let (r, g, b) = hsv_to_rgb(h, s, v);
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
        (a * 255.0).round() as u8
    )
}

/// The alpha checkerboard, so a translucent colour is distinguishable from a
/// pale one.
fn checkerboard(f: &mut lumen_render::canvas::Frame, r: kurbo::Rect, cell: f64) {
    let light = Color::srgb8(0xff, 0xff, 0xff, 0xff);
    let dark = Color::srgb8(0xcf, 0xd4, 0xda, 0xff);
    f.fill_rect(r, lumen_render::Brush::Solid(light));
    let mut y = r.y0;
    let mut row = 0;
    while y < r.y1 {
        let mut x = r.x0 + if row % 2 == 0 { 0.0 } else { cell };
        while x < r.x1 {
            f.fill_rect(
                kurbo::Rect::new(x, y, (x + cell).min(r.x1), (y + cell).min(r.y1)),
                lumen_render::Brush::Solid(dark),
            );
            x += cell * 2.0;
        }
        y += cell;
        row += 1;
    }
}

/// A ring marker that stays visible on any backdrop.
fn marker(f: &mut lumen_render::canvas::Frame, at: kurbo::Point, r: f64) {
    f.fill_circle(at, r + 1.5, Color::srgb8(0x1b, 0x22, 0x30, 0xcc));
    f.fill_circle(at, r, Color::WHITE);
}

impl ColorPicker {
    /// A picker storing the chosen `#rrggbbaa` under `name` (default
    /// `#1a73e8ff`).
    ///
    /// The panel is a full colour editor, not just a palette: a
    /// saturation/value **plane** you drag a marker around, a **hue** bar under
    /// it, an **alpha** bar over a checkerboard, the preset swatches, and the
    /// live hex. The presets alone offered 12 colours and no way to reach
    /// anything between them.
    ///
    /// Working state is `{name}.h`, `.s`, `.v`, `.a` (each `0..=1`); `{name}`
    /// is written on every change, so callers that only read the hex are
    /// unaffected. `{name}.open` is the panel flag.
    pub fn new(cx: &BuildCx, name: &str) -> ColorPicker {
        let value = cx.signal(name, || "#1a73e8ff".to_string());
        let open = cx.signal(format!("{name}.open"), || false);
        let current = value.get(cx.runtime());
        let is_open = open.get(cx.runtime());

        // Seed HSVA from the stored hex the first time, so a caller that only
        // sets `{name}` still opens the panel on the right colour.
        let seed = Color::from_hex(&current).unwrap_or(Color::srgb8(0x1a, 0x73, 0xe8, 0xff));
        let seed_srgb = seed.to_hex();
        let px = |i: usize| {
            u8::from_str_radix(&seed_srgb[1 + i * 2..3 + i * 2], 16).unwrap_or(0) as f64 / 255.0
        };
        let (sh, ss, sv) = rgb_to_hsv(px(0), px(1), px(2));
        let hue = cx.signal(format!("{name}.h"), || sh);
        let sat = cx.signal(format!("{name}.s"), || ss);
        let val = cx.signal(format!("{name}.v"), || sv);
        let alpha = cx.signal(format!("{name}.a"), || px(3));

        let rt = cx.runtime();
        let (h, s_, v, a) = (hue.get(rt), sat.get(rt), val.get(rt), alpha.get(rt));
        let chosen = hsva_color(h, s_, v, a);

        // Any control writing HSVA also republishes the hex.
        let publish = move |rt: &lumen_core::state::Runtime| {
            let hex = hsva_hex(hue.get(rt), sat.get(rt), val.get(rt), alpha.get(rt));
            value.set(rt, hex);
        };

        // Trigger: the current colour as a bordered swatch over a checkerboard.
        let mut trigger = Element::default().class("swatch");
        trigger.role = Role::Button;
        trigger.label = format!("color {current}");
        trigger.focusable = true;
        trigger.background = Some(chosen);
        trigger.cursor = Some(lumen_core::CursorShape::Pointer);
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

            // --- saturation / value plane ----------------------------------
            let mut plane: Element = crate::Canvas::new(PLANE_W, PLANE_H, move |f, size| {
                let r = kurbo::Rect::new(0.0, 0.0, size.width, size.height);
                // White → full hue across, then transparent → black down.
                f.linear_gradient_rect(r, Color::WHITE, hsva_color(h, 1.0, 1.0, 1.0));
                f.vertical_gradient_rect(
                    r,
                    Color::new_linear(0.0, 0.0, 0.0, 0.0),
                    Color::new_linear(0.0, 0.0, 0.0, 1.0),
                );
                marker(
                    f,
                    kurbo::Point::new(s_ * size.width, (1.0 - v) * size.height),
                    5.0,
                );
            })
            .into();
            plane.corner_radius = 6.0;
            plane.role = Role::Slider;
            plane.label = "saturation and value".to_string();
            plane.value = Some(format!("s {:.2} v {:.2}", s_, v));
            plane.cursor = Some(lumen_core::CursorShape::Crosshair);
            plane.id = Some(format!("{name}-plane").into());
            plane.on_drag = Some(Rc::new(move |rt, fx, fy, _| {
                sat.set(rt, fx.clamp(0.0, 1.0));
                val.set(rt, 1.0 - fy.clamp(0.0, 1.0));
                publish(rt);
            }));
            rows.push(plane);

            // --- hue bar ----------------------------------------------------
            let mut hue_bar: Element = crate::Canvas::new(PLANE_W, BAR_H, move |f, size| {
                // Six two-stop segments approximate the full turn.
                const SEGS: usize = 6;
                for i in 0..SEGS {
                    let (x0, x1) = (
                        size.width * i as f64 / SEGS as f64,
                        size.width * (i + 1) as f64 / SEGS as f64,
                    );
                    f.linear_gradient_rect(
                        kurbo::Rect::new(x0, 0.0, x1, size.height),
                        hsva_color(i as f64 / SEGS as f64, 1.0, 1.0, 1.0),
                        hsva_color((i + 1) as f64 / SEGS as f64, 1.0, 1.0, 1.0),
                    );
                }
                marker(
                    f,
                    kurbo::Point::new(h * size.width, size.height / 2.0),
                    size.height / 2.0 - 2.0,
                );
            })
            .into();
            hue_bar.corner_radius = BAR_H / 2.0;
            hue_bar.role = Role::Slider;
            hue_bar.label = "hue".to_string();
            hue_bar.value = Some(format!("{:.0}\u{b0}", h * 360.0));
            hue_bar.id = Some(format!("{name}-hue").into());
            hue_bar.on_drag = Some(Rc::new(move |rt, fx, _, _| {
                hue.set(rt, fx.clamp(0.0, 1.0));
                publish(rt);
            }));
            rows.push(hue_bar);

            // --- alpha bar --------------------------------------------------
            let mut alpha_bar: Element = crate::Canvas::new(PLANE_W, BAR_H, move |f, size| {
                let r = kurbo::Rect::new(0.0, 0.0, size.width, size.height);
                checkerboard(f, r, size.height / 2.0);
                f.linear_gradient_rect(r, hsva_color(h, s_, v, 0.0), hsva_color(h, s_, v, 1.0));
                marker(
                    f,
                    kurbo::Point::new(a * size.width, size.height / 2.0),
                    size.height / 2.0 - 2.0,
                );
            })
            .into();
            alpha_bar.corner_radius = BAR_H / 2.0;
            alpha_bar.role = Role::Slider;
            alpha_bar.label = "alpha".to_string();
            alpha_bar.value = Some(format!("{:.0}%", a * 100.0));
            alpha_bar.id = Some(format!("{name}-alpha").into());
            alpha_bar.on_drag = Some(Rc::new(move |rt, fx, _, _| {
                alpha.set(rt, fx.clamp(0.0, 1.0));
                publish(rt);
            }));
            rows.push(alpha_bar);

            // --- presets ----------------------------------------------------
            for chunk in PRESETS.chunks(8) {
                let cells: Vec<Element> = chunk
                    .iter()
                    .map(|hex| {
                        let hex_s = hex.to_string();
                        let mut c = Element::default().class("cell");
                        c.role = Role::Button;
                        c.label = hex_s.clone();
                        c.focusable = true;
                        c.background = Color::from_hex(hex).ok();
                        c.cursor = Some(lumen_core::CursorShape::Pointer);
                        // Without an edge the white swatch is invisible against
                        // the white panel — a hole where a colour should be.
                        c.border = Some(lumen_render::Border {
                            width: 1.0,
                            color: Color::srgb8(0xd8, 0xdd, 0xe3, 0xff),
                        });
                        c.corner_radius = 4.0;
                        c.style.width = Dim::px(20.0);
                        c.style.height = Dim::px(20.0);
                        c.on_click = Some(Rc::new(move |rt| {
                            // A preset sets the *controls*, so the plane and
                            // bars jump to it and stay editable from there —
                            // it is a starting point, not a terminal choice.
                            if let Ok(col) = Color::from_hex(&hex_s) {
                                let s = col.to_hex();
                                let p = |i: usize| {
                                    u8::from_str_radix(&s[1 + i * 2..3 + i * 2], 16).unwrap_or(0)
                                        as f64
                                        / 255.0
                                };
                                let (hh, sss, vv) = rgb_to_hsv(p(0), p(1), p(2));
                                hue.set(rt, hh);
                                sat.set(rt, sss);
                                val.set(rt, vv);
                                alpha.set(rt, p(3));
                                publish(rt);
                            }
                        }));
                        c
                    })
                    .collect();
                let mut r = widgets::row(cells);
                r.style.column_gap = Dim::px(5.0);
                rows.push(r);
            }

            // --- readout -----------------------------------------------------
            let mut hex_label = widgets::text(hsva_hex(h, s_, v, a)).id(format!("{name}-hex"));
            if let Some(ts) = hex_label.text_style_mut() {
                ts.font_size = 12.0;
                ts.color = Color::srgb8(0x4b, 0x53, 0x60, 0xff);
            }
            let mut swatch = Element::default();
            swatch.background = Some(chosen);
            swatch.border = Some(lumen_render::Border {
                width: 1.0,
                color: Color::srgb8(0xd8, 0xdd, 0xe3, 0xff),
            });
            swatch.corner_radius = 4.0;
            swatch.style.width = Dim::px(20.0);
            swatch.style.height = Dim::px(20.0);
            let mut readout = widgets::row(vec![swatch, hex_label]);
            readout.style.column_gap = Dim::px(8.0);
            readout.style.align_items = Some(lumen_layout::Align::Center);
            rows.push(readout);

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
            panel.style.row_gap = Dim::px(8.0);
            panel.style.padding = Edges::all(Dim::px(10.0));
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
