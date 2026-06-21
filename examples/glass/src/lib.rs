//! glass — Apple-style frosted glass via the `backdrop-filter` primitive. A
//! vivid painted background sits behind a translucent card; the card's
//! `backdrop-filter: blur(...) saturate(...)` (in `app.lss`) blurs + enriches
//! whatever shows through, which is what reads as glass. The pill "chips" use a
//! lighter blur to show the effect composes per-node.
use kurbo::{Point, Rect};
use lumen_core::Color;
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Edges, Position};

/// Build the glass app.
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

/// A full-bleed painted backdrop (so the blur has something vivid to work on):
/// a diagonal ramp with a few saturated blobs.
fn backdrop() -> Element {
    let mut c = widgets::canvas(600.0, 520.0, |f, size| {
        let (w, h) = (size.width, size.height);
        f.linear_gradient_rect(
            Rect::new(0.0, 0.0, w, h),
            Color::srgb8(0x3a, 0x12, 0x6b, 0xff),
            Color::srgb8(0x0b, 0x2a, 0x5b, 0xff),
        );
        let blob = |f: &mut lumen_render::canvas::Frame, fx: f64, fy: f64, r: f64, c: Color| {
            f.fill_circle(Point::new(w * fx, h * fy), r, c);
        };
        blob(f, 0.18, 0.22, 130.0, Color::srgb8(0xff, 0x3d, 0x9a, 0xff));
        blob(f, 0.82, 0.30, 150.0, Color::srgb8(0x21, 0xd4, 0xfd, 0xff));
        blob(f, 0.30, 0.85, 140.0, Color::srgb8(0xff, 0xa6, 0x2b, 0xff));
        blob(f, 0.78, 0.82, 110.0, Color::srgb8(0x7c, 0xf2, 0x6b, 0xff));
    });
    c.style.position = Position::Absolute;
    c.style.inset = Edges::all(Dim::px(0.0));
    c.style.width = Dim::pct(1.0);
    c.style.height = Dim::pct(1.0);
    c
}

fn chip(label: &str) -> Element {
    let mut e = txt(label, 13.0, 600.0).class("chip");
    e.style.padding = Edges {
        left: Dim::px(14.0),
        right: Dim::px(14.0),
        top: Dim::px(7.0),
        bottom: Dim::px(7.0),
    };
    e
}

fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;

    let chips = {
        let mut r = widgets::row(vec![chip("Frosted"), chip("Vibrant"), chip("Live")]);
        r.style.column_gap = Dim::px(10.0);
        r.style.justify_content = Some(Align::Center);
        r
    };

    let mut card = widgets::column(vec![
        txt("Liquid Glass", 30.0, 800.0).class("title"),
        txt(
            "A translucent panel blurring the colour behind it.",
            15.0,
            400.0,
        )
        .class("subtitle"),
        chips,
    ])
    .class("glass")
    .id("card");
    card.style.align_items = Some(Align::Center);
    card.style.row_gap = Dim::px(16.0);
    card.style.padding = Edges::all(Dim::px(34.0));
    card.style.width = Dim::px(380.0);
    card.shadow = Some(Shadow::soft());

    // Card sits in normal flow, centred; backdrop is absolute behind it. The card
    // is painted after the backdrop (document order) so its filter can read it.
    let mut page = widgets::column(vec![backdrop(), card]).id("page");
    page.style.position = Position::Relative;
    page.style.width = Dim::pct(1.0);
    page.style.height = Dim::pct(1.0);
    page.style.align_items = Some(Align::Center);
    page.style.justify_content = Some(Align::Center);
    page
}
