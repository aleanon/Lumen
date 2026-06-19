//! gradient — a hero linear gradient plus a row of gradient chips (Oklab-
//! interpolated). Chrome themed from `app.lss`; gradient colours are code.
use kurbo::Rect;
use lumen_core::Color;
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};

/// Build the gradient app.
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

fn strip(w: f64, h: f64, a: Color, b: Color) -> Element {
    widgets::canvas(w, h, move |f, size| {
        f.linear_gradient_rect(Rect::new(0.0, 0.0, size.width, size.height), a, b);
    })
}

fn chip(name: &str, a: Color, b: Color) -> Element {
    let mut c = widgets::column(vec![
        strip(96.0, 64.0, a, b),
        txt(name, 11.0, 600.0).class("chip-name"),
    ]);
    c.style.row_gap = Dim::px(6.0);
    c.style.align_items = Some(Align::Center);
    c
}

fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;
    let hero = strip(
        400.0,
        150.0,
        Color::srgb8(0x29, 0x6b, 0xff, 0xff),
        Color::srgb8(0xe8, 0x1a, 0x4b, 0xff),
    )
    .id("hero");

    let chips = {
        let mut r = widgets::row(vec![
            chip(
                "Ocean",
                Color::srgb8(0x16, 0xa3, 0xff, 0xff),
                Color::srgb8(0x12, 0xd9, 0xa8, 0xff),
            ),
            chip(
                "Sunset",
                Color::srgb8(0xff, 0x8a, 0x00, 0xff),
                Color::srgb8(0xe8, 0x1a, 0x4b, 0xff),
            ),
            chip(
                "Grape",
                Color::srgb8(0x7c, 0x4d, 0xff, 0xff),
                Color::srgb8(0xff, 0x5a, 0x9e, 0xff),
            ),
            chip(
                "Lime",
                Color::srgb8(0x9a, 0xe6, 0x00, 0xff),
                Color::srgb8(0x12, 0xc2, 0x6a, 0xff),
            ),
        ]);
        r.style.column_gap = Dim::px(12.0);
        r.style.justify_content = Some(Align::Center);
        r
    };

    let mut card = widgets::column(vec![
        txt("Gradients", 24.0, 800.0).class("title").id("title"),
        txt("Two-stop ramps, interpolated in Oklab.", 14.0, 400.0).class("subtitle"),
        hero,
        chips,
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
