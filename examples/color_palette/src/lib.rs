//! color_palette — a hue slider drives a computed tonal ramp (OkLCh). Chrome
//! themed from `app.lss`; the swatch colours are computed in code.
use lumen_core::Color;
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};

/// Build the color-palette app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

fn txt(s: impl Into<String>, size: f32, weight: f32) -> Element {
    let mut e = widgets::text(s);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = size;
        ts.weight = weight;
    }
    e
}

fn box_(color: Color, w: f32, h: f32, radius: f64) -> Element {
    let mut e = Element {
        background: Some(color),
        style: LayoutStyle {
            width: Dim::px(w),
            height: Dim::px(h),
            ..LayoutStyle::default()
        },
        ..Element::default()
    };
    e.corner_radius = radius;
    e
}

fn build(cx: &mut BuildCx) -> Element {
    let hue = cx.signal("hue", || 255.0f64);
    let h = hue.get(cx.runtime());
    let base = Color::from_oklch(0.62, 0.15, h as f32);

    // Big base swatch + hex.
    let hero = {
        let mut c = widgets::column(vec![
            box_(base, 320.0, 96.0, 16.0),
            txt(base.to_hex(), 15.0, 600.0).class("hex"),
        ]);
        c.style.row_gap = Dim::px(8.0);
        c.style.align_items = Some(Align::Center);
        c
    };

    // Tonal ramp across lightness at the chosen hue.
    let ramp = {
        let levels = [0.92, 0.80, 0.68, 0.56, 0.44, 0.33, 0.23];
        let chips: Vec<Element> = levels
            .iter()
            .map(|&l| {
                let col = Color::from_oklch(l, 0.13, h as f32);
                let mut c = widgets::column(vec![
                    box_(col, 64.0, 52.0, 10.0),
                    txt(col.to_hex(), 10.0, 500.0).class("hex"),
                ]);
                c.style.row_gap = Dim::px(5.0);
                c.style.align_items = Some(Align::Center);
                c
            })
            .collect();
        let mut r = widgets::row(chips);
        r.style.column_gap = Dim::px(8.0);
        r.style.justify_content = Some(Align::Center);
        r
    };

    let slider = {
        let mut s = widgets::slider(cx, "hue", 0.0, 360.0).id("hue");
        s.style.width = Dim::px(320.0);
        s
    };

    let mut card = widgets::column(vec![
        txt("Palette", 24.0, 800.0).class("title").id("title"),
        txt(format!("Hue {:.0}° — drag to explore", h), 14.0, 400.0).class("subtitle"),
        hero,
        slider,
        ramp,
    ])
    .id("card");
    card.style.align_items = Some(Align::Center);
    card.style.row_gap = Dim::px(18.0);
    card.style.padding = Edges::all(Dim::px(30.0));
    card.shadow = Some(Shadow::soft());

    Element {
        role: lumen_core::semantics::Role::Group,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Dim::pct(1.0),
            height: Dim::pct(1.0),
            align_items: Some(Align::Center),
            justify_content: Some(Align::Center),
            ..LayoutStyle::default()
        },
        children: vec![card],
        ..Element::default()
    }
    .id("page")
}
