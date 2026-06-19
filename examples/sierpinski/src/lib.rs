//! sierpinski — a Sierpinski-triangle fractal (canvas) with a depth control.
//! Chrome themed from `app.lss`; the fractal is drawn procedurally.
use kurbo::{BezPath, Point};
use lumen_core::state::Runtime;
use lumen_core::Color;
use lumen_render::canvas::Frame;
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};

/// Build the sierpinski app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

const SIDE: f64 = 360.0;

fn mid(a: Point, b: Point) -> Point {
    Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}

fn tri(f: &mut Frame, a: Point, b: Point, c: Point, depth: u32, color: Color) {
    if depth == 0 {
        let mut p = BezPath::new();
        p.move_to(a);
        p.line_to(b);
        p.line_to(c);
        p.close_path();
        f.fill(&p, color);
        return;
    }
    tri(f, a, mid(a, b), mid(a, c), depth - 1, color);
    tri(f, mid(a, b), b, mid(b, c), depth - 1, color);
    tri(f, mid(a, c), mid(b, c), c, depth - 1, color);
}

fn txt(s: impl Into<String>, size: f32, weight: f32) -> Element {
    let mut e = widgets::text(s);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = size;
        ts.weight = weight;
    }
    e
}

fn pad(mut e: Element, h: f32, v: f32) -> Element {
    e.style.padding = Edges {
        left: Dim::px(h),
        right: Dim::px(h),
        top: Dim::px(v),
        bottom: Dim::px(v),
    };
    e
}

fn button(label: &str, on: impl Fn(&Runtime) + 'static) -> Element {
    let mut e = widgets::button(label, on).class("btn");
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = 20.0;
        ts.weight = 700.0;
    }
    pad(e, 18.0, 8.0)
}

fn build(cx: &mut BuildCx) -> Element {
    let depth = cx.signal("depth", || 5i64);
    let d = depth.get(cx.runtime()).clamp(0, 9) as u32;

    let face = widgets::canvas(SIDE, SIDE * 0.9, move |f, size| {
        let apex = Point::new(size.width / 2.0, 8.0);
        let bl = Point::new(8.0, size.height - 8.0);
        let br = Point::new(size.width - 8.0, size.height - 8.0);
        tri(f, apex, br, bl, d, Color::srgb8(0x7c, 0x8c, 0xff, 0xff));
    })
    .id("face");

    let controls = {
        let mut r = widgets::row(vec![
            button("−", move |rt| depth.update(rt, |v| *v = (*v - 1).max(0))).id("dec"),
            pad(
                txt(format!("depth {d}"), 16.0, 700.0).class("depth"),
                12.0,
                0.0,
            ),
            button("+", move |rt| depth.update(rt, |v| *v = (*v + 1).min(9))).id("inc"),
        ]);
        r.style.column_gap = Dim::px(12.0);
        r.style.align_items = Some(Align::Center);
        r.style.justify_content = Some(Align::Center);
        r
    };

    let mut card = widgets::column(vec![
        txt("Sierpinski", 22.0, 800.0).class("title").id("title"),
        face,
        controls,
    ])
    .id("card");
    card.style.align_items = Some(Align::Center);
    card.style.row_gap = Dim::px(16.0);
    card.style.padding = Edges::all(Dim::px(28.0));
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
