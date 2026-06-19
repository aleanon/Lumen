//! loading_spinners — three spinners animating off the virtual clock (rotating
//! arcs on a faint track). Chrome themed from `app.lss`; arcs are code colours.
use kurbo::{Arc, Circle, Point, Shape, Vec2};
use lumen_core::Color;
use lumen_render::canvas::Frame;
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};

/// Build the loading-spinners app.
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

fn spinner(t: f64, speed: f64, color: Color) -> impl Fn(&mut Frame, lumen_core::geometry::Size) {
    move |f: &mut Frame, size| {
        let c = Point::new(size.width / 2.0, size.height / 2.0);
        let r = size.width.min(size.height) / 2.0 - 6.0;
        // faint full-circle track
        f.stroke(
            &Circle::new(c, r).to_path(0.1),
            Color::srgb8(0x26, 0x30, 0x44, 0xff),
            6.0,
        );
        // rotating ~280° arc
        let start = (t * speed) % std::f64::consts::TAU;
        let arc =
            Arc::new(c, Vec2::new(r, r), start, std::f64::consts::TAU * 0.78, 0.0).to_path(0.1);
        f.stroke(&arc, color, 6.0);
    }
}

fn cell(name: &str, draw: impl Fn(&mut Frame, lumen_core::geometry::Size) + 'static) -> Element {
    let mut c = widgets::column(vec![
        widgets::canvas(72.0, 72.0, draw),
        txt(name, 12.0, 600.0).class("name"),
    ]);
    c.style.row_gap = Dim::px(10.0);
    c.style.align_items = Some(Align::Center);
    c
}

fn build(cx: &mut BuildCx) -> Element {
    cx.animate();
    let t = cx.now_ms() / 1000.0;

    let row = {
        let mut r = widgets::row(vec![
            cell(
                "Cyan",
                spinner(t, 4.0, Color::srgb8(0x38, 0xbd, 0xf8, 0xff)),
            ),
            cell(
                "Violet",
                spinner(t, 6.0, Color::srgb8(0xa7, 0x8b, 0xfa, 0xff)),
            ),
            cell(
                "Rose",
                spinner(t, 8.0, Color::srgb8(0xfb, 0x71, 0x85, 0xff)),
            ),
        ]);
        r.style.column_gap = Dim::px(28.0);
        r.style.justify_content = Some(Align::Center);
        r
    };

    let mut card = widgets::column(vec![
        txt("Loading", 22.0, 800.0).class("title").id("title"),
        row,
    ])
    .id("card");
    card.style.align_items = Some(Align::Center);
    card.style.row_gap = Dim::px(22.0);
    card.style.padding = Edges::all(Dim::px(34.0));
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
