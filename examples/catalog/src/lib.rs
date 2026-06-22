//! catalog — a virtualized 1000-row list whose rows fetch their data
//! asynchronously, mocked with a per-row random delay. A row that is still
//! fetching shows a spinner; a resolved row renders its record. Theme: a survey
//! of the **Gaia** stellar catalog (1000 stars, each with a spectral class,
//! distance, and magnitude).
//!
//! - **Virtualized:** only the ~visible rows exist in the tree, so layout stays
//!   cheap and only on-screen rows fetch (lazy loading — scroll to load more).
//! - **Async via the data layer:** each visible row is a `cx.resource_blocking`
//!   keyed by its index; the fetcher sleeps a deterministic-random duration on a
//!   pool thread, then returns the record. Resolved rows are cached by key, so
//!   scrolling back is instant.
//! - **Spinner:** an animated arc drawn on a canvas while a row loads; the app
//!   only animates while something on screen is still loading.
use kurbo::{Arc, Circle, Point, Shape, Vec2};
use lumen_core::Color;
use lumen_render::canvas::Frame;
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element, TaskError};

use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};
use std::time::Duration;

/// Build the catalog app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

const TOTAL: usize = 1000;
const ROW_H: f64 = 46.0;
const VISIBLE: usize = 11;
const VIEW_H: f64 = ROW_H * VISIBLE as f64;

/// A record for one star (what the async fetch resolves to). A tuple keeps it a
/// `State` type without a serde derive in this crate: (designation, distance ly,
/// spectral-class index 0..7, magnitude×100).
type Star = (String, u32, u8, u16);

/// Deterministic hash → the per-row "random" data and delay are stable per index
/// (so the same star always looks/loads the same), but vary across rows.
fn splitmix(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Synthesize a star record for row `i`.
fn star_of(i: u64) -> Star {
    let h = splitmix(i);
    let designation = format!("GAIA DR3 {}", 1_000_000_000 + h % 8_999_999_999);
    let distance = (4 + (h >> 8) % 3600) as u32; // 4..3604 ly
    let class = ((h >> 22) % 7) as u8; // O B A F G K M
    let mag = ((h >> 28) % 900) as u16; // 0.00 .. 9.00 ×100
    (designation, distance, class, mag)
}

/// Per-row mocked latency (20..240 ms), deterministic per index.
fn delay_ms(i: u64) -> u64 {
    20 + (splitmix(i ^ 0xABCD).wrapping_mul(2862933555777941757) >> 40) % 220
}

const CLASSES: [&str; 7] = ["O", "B", "A", "F", "G", "K", "M"];

/// The Harvard spectral-class colour (hot blue → cool red).
fn class_color(c: u8) -> Color {
    match c {
        0 => Color::srgb8(0x9b, 0xb0, 0xff, 0xff), // O
        1 => Color::srgb8(0xc9, 0xd6, 0xff, 0xff), // B
        2 => Color::srgb8(0xf0, 0xf3, 0xff, 0xff), // A
        3 => Color::srgb8(0xf8, 0xf7, 0xe6, 0xff), // F
        4 => Color::srgb8(0xff, 0xe9, 0x9a, 0xff), // G
        5 => Color::srgb8(0xff, 0xc2, 0x6b, 0xff), // K
        _ => Color::srgb8(0xff, 0x8a, 0x5b, 0xff), // M
    }
}

fn txt(s: impl Into<String>, size: f32, weight: f32) -> Element {
    let mut e = widgets::text(s);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = size;
        ts.weight = weight;
    }
    e
}

/// A small rotating-arc spinner on a faint track (driven by `now_ms`).
fn spinner(t: f64) -> Element {
    widgets::canvas(20.0, 20.0, move |f: &mut Frame, size| {
        let c = Point::new(size.width / 2.0, size.height / 2.0);
        let r = size.width.min(size.height) / 2.0 - 2.5;
        f.stroke(
            &Circle::new(c, r).to_path(0.1),
            Color::srgb8(0x22, 0x2c, 0x44, 0xff),
            2.5,
        );
        let start = (t * 5.0) % std::f64::consts::TAU;
        let arc =
            Arc::new(c, Vec2::new(r, r), start, std::f64::consts::TAU * 0.72, 0.0).to_path(0.1);
        f.stroke(&arc, Color::srgb8(0x3a, 0x86, 0xff, 0xff), 2.5);
    })
}

/// A solid colour dot (the resolved star's spectral class).
fn dot(color: Color) -> Element {
    let [r, g, b, _] = color.to_srgb8();
    let glow = Color::srgb8(r, g, b, 0x40);
    widgets::canvas(20.0, 20.0, move |f: &mut Frame, size| {
        let c = Point::new(size.width / 2.0, size.height / 2.0);
        // a soft glow ring, then the solid dot
        f.stroke(
            &Circle::new(c, size.width / 2.0 - 1.5).to_path(0.1),
            glow,
            2.0,
        );
        f.fill_circle(c, size.width.min(size.height) / 2.0 - 3.5, color);
    })
}

/// One catalog row, fixed at `ROW_H` so the viewport is an exact multiple (no
/// clipping needed). Shows a spinner while loading, the record once resolved.
fn row_view(idx: usize, value: &Option<Star>, t: f64, alt: bool) -> Element {
    let mut index = txt(format!("{:>4}", idx + 1), 12.0, 600.0).class("idx");
    index.style.width = Dim::px(40.0);

    let (lead, body): (Element, Element) = match value {
        Some((name, dist, class, mag)) => {
            let lead = dot(class_color(*class));
            let cls = CLASSES[(*class as usize).min(6)];
            let mut name_el = txt(name.clone(), 13.0, 600.0).class("name");
            name_el.style.flex_grow = 1.0;
            let mut meta = txt(
                format!("{cls}  ·  {dist} ly  ·  m{:.2}", *mag as f32 / 100.0),
                12.0,
                500.0,
            )
            .class("meta");
            meta.style.width = Dim::px(168.0);
            let mut b = widgets::row(vec![name_el, meta]);
            b.style.flex_grow = 1.0;
            b.style.column_gap = Dim::px(10.0);
            b.style.align_items = Some(Align::Center);
            (lead, b)
        }
        None => {
            let mut l = txt("acquiring signal…", 12.0, 500.0).class("loading");
            l.style.flex_grow = 1.0;
            (spinner(t), l)
        }
    };

    let mut r = widgets::row(vec![index, lead, body]).class(if alt { "row-alt" } else { "row" });
    r.style.height = Dim::px(ROW_H as f32);
    r.style.column_gap = Dim::px(12.0);
    r.style.align_items = Some(Align::Center);
    r.style.padding = Edges {
        left: Dim::px(16.0),
        right: Dim::px(16.0),
        top: Dim::px(0.0),
        bottom: Dim::px(0.0),
    };
    r
}

fn build(cx: &mut BuildCx) -> Element {
    let scroll = cx.signal("scroll", || 0.0f64);
    let max_y = (TOTAL - VISIBLE) as f64 * ROW_H;
    let y = scroll.get(cx.runtime()).clamp(0.0, max_y);
    let first = ((y / ROW_H).floor() as usize).min(TOTAL - VISIBLE);

    // Build only the visible window; each visible row lazily fetches its record.
    let mut rows = Vec::with_capacity(VISIBLE);
    let mut any_loading = false;
    let mut loaded_visible = 0usize;
    for idx in first..first + VISIBLE {
        let star = cx.resource_blocking::<Star, TaskError, _>(
            &format!("star-{idx}"),
            idx as u64,
            move |i| {
                // Mock an async fetch: sleep a per-row random duration on a pool
                // thread, then return the record (transport is irrelevant here —
                // the data layer feeds the result back into state).
                std::thread::sleep(Duration::from_millis(delay_ms(i)));
                Ok(star_of(i))
            },
        );
        if star.value.is_some() {
            loaded_visible += 1;
        } else {
            any_loading = true;
        }
        rows.push(row_view(
            idx,
            &star.value,
            cx.now_ms() / 1000.0,
            idx % 2 == 1,
        ));
    }
    // Only animate while something on screen is still loading (idle otherwise).
    if any_loading {
        cx.animate();
    }

    // The viewport: exactly VISIBLE rows tall, wheel scrolls the window.
    let list = {
        let mut c = widgets::column(rows);
        c.style.row_gap = Dim::px(0.0);
        c
    };
    let viewport = Element {
        role: lumen_core::semantics::Role::Group,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            height: Dim::px(VIEW_H as f32),
            width: Dim::pct(1.0),
            ..LayoutStyle::default()
        },
        on_wheel: Some(std::rc::Rc::new(move |rt, dy| {
            scroll.update(rt, |o| *o = (*o + dy).clamp(0.0, max_y))
        })),
        children: vec![list],
        ..Element::default()
    }
    .id("viewport");

    // A slim scrollbar indicator beside the list.
    let bar = {
        let frac_h = (VIEW_H / (TOTAL as f64 * ROW_H)).clamp(0.04, 1.0);
        let thumb_h = (VIEW_H * frac_h).max(24.0);
        let thumb_y = (y / max_y.max(1.0)) * (VIEW_H - thumb_h);
        let mut thumb = Element::default().class("scrollbar-thumb");
        thumb.style.width = Dim::px(6.0);
        thumb.style.height = Dim::px(thumb_h as f32);
        thumb.style.margin.top = Dim::px(thumb_y as f32);
        let mut track = widgets::column(vec![thumb]).class("scrollbar-track");
        track.style.width = Dim::px(6.0);
        track.style.height = Dim::px(VIEW_H as f32);
        track
    };

    let body = {
        let mut r = widgets::row(vec![viewport, bar]);
        r.style.column_gap = Dim::px(8.0);
        r.style.width = Dim::pct(1.0);
        r
    };

    // Header: title + a live "loaded N / 1000 in view" counter.
    let header = {
        let title = widgets::column(vec![
            txt("Gaia Stellar Survey", 20.0, 800.0).class("title"),
            txt(
                format!(
                    "Rows {}–{} of {TOTAL}  ·  scroll to load",
                    first + 1,
                    first + VISIBLE
                ),
                12.0,
                500.0,
            )
            .class("subtitle"),
        ]);
        let status = if any_loading {
            txt(format!("{loaded_visible}/{VISIBLE} in view"), 12.0, 700.0).class("loading")
        } else {
            txt("all in view loaded", 12.0, 700.0).class("count")
        };
        let mut h = widgets::row(vec![title, status]).id("header");
        h.style.align_items = Some(Align::Center);
        h.style.justify_content = Some(Align::SpaceBetween);
        h.style.width = Dim::pct(1.0);
        h.style.padding = Edges::all(Dim::px(16.0));
        h
    };

    let mut card = widgets::column(vec![header, body]).id("card");
    card.style.row_gap = Dim::px(12.0);
    card.style.padding = Edges::all(Dim::px(16.0));
    card.style.width = Dim::px(480.0);
    card.shadow = Some(Shadow::soft());

    let mut page = widgets::column(vec![card]).id("page");
    page.style.width = Dim::pct(1.0);
    page.style.height = Dim::pct(1.0);
    page.style.align_items = Some(Align::Center);
    page.style.justify_content = Some(Align::Center);
    page
}
