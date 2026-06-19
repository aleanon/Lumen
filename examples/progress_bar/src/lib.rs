//! progress_bar — labelled status bars in the theme's colours plus one animated
//! looping bar. Track/fill colours come from `app.lss`; fill widths are layout.
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};

/// Build the progress-bar app.
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

fn fill(mut e: Element) -> Element {
    e.style.flex_grow = 1.0;
    e.style.min_width = Dim::px(0.0);
    e
}

fn bar(name: &str, value: f64, color: &str) -> Element {
    let v = value.clamp(0.0, 1.0);
    let mut fill_el = Element {
        style: LayoutStyle {
            width: Dim::pct(v as f32),
            height: Dim::pct(1.0),
            ..LayoutStyle::default()
        },
        ..Element::default()
    }
    .class("fill")
    .class(color);
    fill_el.corner_radius = 999.0;

    let mut track = Element {
        style: LayoutStyle {
            width: Dim::pct(1.0),
            height: Dim::px(12.0),
            ..LayoutStyle::default()
        },
        children: vec![fill_el],
        ..Element::default()
    }
    .class("track");
    track.corner_radius = 999.0;

    let mut head = widgets::row(vec![
        fill(txt(name, 14.0, 600.0).class("name")),
        txt(format!("{:.0}%", v * 100.0), 13.0, 600.0).class("pct"),
    ]);
    head.style.align_items = Some(Align::Center);

    let mut col = widgets::column(vec![head, track]);
    col.style.row_gap = Dim::px(7.0);
    col.style.width = Dim::pct(1.0);
    col
}

fn build(cx: &mut BuildCx) -> Element {
    cx.animate();
    let loop_v = (cx.now_ms() / 2600.0) % 1.0;

    let mut card = widgets::column(vec![
        txt("System", 24.0, 800.0).class("title").id("title"),
        bar("Storage", 0.72, "danger"),
        bar("Bandwidth", 0.45, "info"),
        bar("Coverage", 0.90, "success"),
        bar("Battery", 0.60, "warn"),
        bar("Downloading…", loop_v, "accent"),
    ])
    .id("card");
    card.style.width = Dim::px(420.0);
    card.style.padding = Edges::all(Dim::px(30.0));
    card.style.row_gap = Dim::px(20.0);
    card.style.align_items = Some(Align::Stretch);
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
