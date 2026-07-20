//! image — an image viewer: a large hero frame from a generated video source
//! plus a filmstrip of thumbnails sampled across time. Stands in for decoded
//! assets; chrome is themed from `app.lss`.
use lumen_render::media::{TestPattern, VideoSource};
use lumen_widgets::element::Shadow;
use lumen_widgets::{asset, widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Edges};

/// Build the image app.
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

/// A framed image: the decoded bitmap on a rounded matte.
fn framed(img: lumen_render::RgbaImage, class: &str, pad: f32) -> Element {
    framed_el(widgets::image(img), class, pad)
}

/// Frame an already-built element (M.1: decoded/animated assets).
fn framed_el(el: Element, class: &str, pad: f32) -> Element {
    let mut f = widgets::column(vec![el]).class(class);
    f.style.padding = Edges::all(Dim::px(pad));
    f.style.align_items = Some(Align::Center);
    f
}

// M.1 (ADR-M1): decoded-asset formats — PNG rides tiny-skia; jpeg/webp/gif
// come from the feature-gated `image` codecs; the GIF plays on the clock.
const FERRIS_JPG: &[u8] = include_bytes!("../assets/ferris.jpg");
const FERRIS_WEBP: &[u8] = include_bytes!("../assets/ferris.webp");
const FERRIS_GIF: &[u8] = include_bytes!("../assets/ferris.gif");

fn build(cx: &mut BuildCx) -> Element {
    let hero = framed(TestPattern.frame_at(0.5, 380, 200), "hero", 6.0).id("photo");

    // Codec row: the same ferris through three decoders + the animated one.
    let codecs = {
        let mut r = widgets::row(vec![
            framed_el(asset::image_any(FERRIS_JPG).id("jpg"), "thumb", 4.0),
            framed_el(asset::image_any(FERRIS_WEBP).id("webp"), "thumb", 4.0),
            framed_el(asset::animated(cx, FERRIS_GIF).id("gif"), "thumb", 4.0),
        ]);
        r.style.column_gap = Dim::px(8.0);
        r
    };

    let strip = {
        let frames: Vec<Element> = [0.0_f64, 0.25, 0.5, 0.75, 1.0]
            .iter()
            .map(|&t| framed(TestPattern.frame_at(t, 84, 54), "thumb", 4.0))
            .collect();
        let mut r = widgets::row(frames);
        r.style.column_gap = Dim::px(10.0);
        r.style.justify_content = Some(Align::Center);
        r
    };

    let mut card = widgets::column(vec![
        txt("Image viewer", 24.0, 800.0).class("title").id("title"),
        txt("A generated source, sampled across time.", 14.0, 400.0).class("subtitle"),
        hero,
        strip,
        txt("Codecs: jpeg · webp · animated gif (M.1)", 12.0, 500.0).class("meta"),
        codecs,
        txt("380 × 200 · RGBA8 · 5 frames", 12.0, 500.0).class("meta"),
    ])
    .id("card");
    card.style.align_items = Some(Align::Center);
    card.style.row_gap = Dim::px(16.0);
    card.style.padding = Edges::all(Dim::px(30.0));
    card.shadow = Some(Shadow::soft());

    let mut page = widgets::column(vec![card]).id("page");
    page.style.width = Dim::pct(1.0);
    page.style.height = Dim::pct(1.0);
    page.style.align_items = Some(Align::Center);
    page.style.justify_content = Some(Align::Center);
    page
}
