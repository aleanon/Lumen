//! svg — a small gallery of vector icons rendered by the deterministic SVG
//! rasterizer (rect/circle/path, solid fill). Each icon is rasterized over the
//! tile colour so its antialiased edges blend; chrome is themed from `app.lss`.
use lumen_core::Color;
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Edges};

/// Build the svg app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

/// Tile background — icons are rasterized over this so AA edges blend in.
fn tile_bg() -> Color {
    Color::srgb8(0xf3, 0xf5, 0xf9, 0xff)
}

fn txt(s: impl Into<String>, size: f32, weight: f32) -> Element {
    let mut e = widgets::text(s);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = size;
        ts.weight = weight;
    }
    e
}

/// Rasterize one SVG string into a named, tiled cell.
fn icon(svg: &str, name: &str) -> Element {
    let img = lumen_render::svg::render(svg, 64, 64, tile_bg());

    let mut tile = widgets::column(vec![widgets::image(img)]).class("tile");
    tile.style.padding = Edges::all(Dim::px(14.0));
    tile.style.align_items = Some(Align::Center);
    tile.style.justify_content = Some(Align::Center);

    let mut cell = widgets::column(vec![tile, txt(name, 12.0, 600.0).class("name")]);
    cell.style.row_gap = Dim::px(8.0);
    cell.style.align_items = Some(Align::Center);
    cell
}

fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;

    let badge = "<svg width=\"64\" height=\"64\">\
        <rect x=\"8\" y=\"8\" width=\"48\" height=\"48\" fill=\"#2ea043\"/>\
        <path d=\"M22 33 L29 40 L43 24 L40 21 L29 34 L25 30 Z\" fill=\"#ffffff\"/>\
        </svg>";
    let heart = "<svg width=\"64\" height=\"64\">\
        <path d=\"M32 54 L12 34 C4 26 8 14 18 14 C25 14 30 19 32 23 C34 19 39 14 46 14 C56 14 60 26 52 34 Z\" fill=\"#e5484d\"/>\
        </svg>";
    let diamond = "<svg width=\"64\" height=\"64\">\
        <path d=\"M32 8 L56 32 L32 56 L8 32 Z\" fill=\"#3b82f6\"/>\
        </svg>";
    let target = "<svg width=\"64\" height=\"64\">\
        <circle cx=\"32\" cy=\"32\" r=\"24\" fill=\"#7c4dff\"/>\
        <circle cx=\"32\" cy=\"32\" r=\"15\" fill=\"#ffffff\"/>\
        <circle cx=\"32\" cy=\"32\" r=\"7\" fill=\"#7c4dff\"/>\
        </svg>";

    let row = {
        let mut r = widgets::row(vec![
            icon(badge, "Check"),
            icon(heart, "Heart"),
            icon(diamond, "Gem"),
            icon(target, "Target"),
        ]);
        r.style.column_gap = Dim::px(16.0);
        r.style.justify_content = Some(Align::Center);
        r
    };

    let mut card = widgets::column(vec![
        txt("SVG icons", 24.0, 800.0).class("title"),
        txt(
            "Rasterized by the deterministic vector renderer.",
            14.0,
            400.0,
        )
        .class("subtitle"),
        row,
    ])
    .id("card");
    card.style.align_items = Some(Align::Center);
    card.style.row_gap = Dim::px(20.0);
    card.style.padding = Edges::all(Dim::px(30.0));
    card.shadow = Some(Shadow::soft());

    let mut page = widgets::column(vec![card]).id("page");
    page.style.width = Dim::pct(1.0);
    page.style.height = Dim::pct(1.0);
    page.style.align_items = Some(Align::Center);
    page.style.justify_content = Some(Align::Center);
    page
}
