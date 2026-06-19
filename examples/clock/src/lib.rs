//! clock — an analog face (canvas) plus a digital readout, ticking off the
//! virtual clock. Card/text themed from `app.lss`; the face is drawn
//! procedurally (canvas colours are code, matched to the theme).
use kurbo::{BezPath, Circle, Point, Shape};
use lumen_core::Color;
use lumen_render::canvas::Frame;
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};

/// Build the clock app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

const DIAL: f64 = 230.0;

fn hand(f: &mut Frame, c: Point, angle: f64, len: f64, width: f64, color: Color) {
    let mut p = BezPath::new();
    p.move_to(c);
    p.line_to(Point::new(c.x + angle.cos() * len, c.y + angle.sin() * len));
    f.stroke(&p, color, width);
}

fn draw_clock(f: &mut Frame, h: f64, m: f64, s: f64) {
    let c = Point::new(DIAL / 2.0, DIAL / 2.0);
    let r = DIAL / 2.0 - 6.0;
    f.fill_circle(c, r, Color::srgb8(0x10, 0x16, 0x28, 0xff));
    f.stroke(
        &Circle::new(c, r).to_path(0.1),
        Color::srgb8(0x2a, 0x36, 0x52, 0xff),
        3.0,
    );

    // hour ticks
    let mut ticks = BezPath::new();
    for i in 0..12 {
        let a = -std::f64::consts::FRAC_PI_2 + (i as f64) * std::f64::consts::TAU / 12.0;
        let (ca, sa) = (a.cos(), a.sin());
        ticks.move_to(Point::new(c.x + ca * (r - 6.0), c.y + sa * (r - 6.0)));
        ticks.line_to(Point::new(c.x + ca * (r - 16.0), c.y + sa * (r - 16.0)));
    }
    f.stroke(&ticks, Color::srgb8(0x46, 0x54, 0x6e, 0xff), 3.0);

    let th =
        |v: f64, p: f64| -> f64 { -std::f64::consts::FRAC_PI_2 + (v / p) * std::f64::consts::TAU };
    hand(
        f,
        c,
        th(h + m / 60.0, 12.0),
        r * 0.5,
        6.0,
        Color::srgb8(0xe8, 0xed, 0xf6, 0xff),
    );
    hand(
        f,
        c,
        th(m + s / 60.0, 60.0),
        r * 0.72,
        4.0,
        Color::srgb8(0xc2, 0xcc, 0xe0, 0xff),
    );
    hand(
        f,
        c,
        th(s, 60.0),
        r * 0.82,
        2.0,
        Color::srgb8(0xff, 0x5a, 0x7a, 0xff),
    );
    f.fill_circle(c, 6.0, Color::srgb8(0xff, 0x5a, 0x7a, 0xff));
}

fn build(cx: &mut BuildCx) -> Element {
    let now = cx.now_ms();
    cx.animate(); // keep ticking
    let secs = now / 1000.0;
    let s = secs % 60.0;
    let m = (secs / 60.0) % 60.0;
    let h = (secs / 3600.0) % 12.0;
    let digital = format!("{:02}:{:02}:{:02}", h as i64, m as i64, s as i64);

    let face = widgets::canvas(DIAL, DIAL, move |f, _size| draw_clock(f, h, m, s)).id("face");

    let mut readout = widgets::text(digital);
    if let Some(ts) = readout.text_style_mut() {
        ts.font_size = 40.0;
        ts.weight = 800.0;
    }

    let mut card = widgets::column(vec![face, readout.class("digital").id("digital"), {
        let mut l = widgets::text("RUNNING CLOCK");
        if let Some(ts) = l.text_style_mut() {
            ts.font_size = 12.0;
            ts.weight = 700.0;
        }
        l.class("label")
    }])
    .id("card");
    card.style.align_items = Some(Align::Center);
    card.style.row_gap = Dim::px(14.0);
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
