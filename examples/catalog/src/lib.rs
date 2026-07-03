//! catalog — a virtualized 1000-row list whose rows fetch their data
//! asynchronously, mocked with a per-row random delay. A row that is still
//! fetching shows a spinner; a resolved row renders its record. Theme: a survey
//! of the **Gaia** stellar catalog (1000 stars, each with a spectral class,
//! distance, and magnitude).
//!
//! - **Virtualized + smooth:** only the visible window (+1 row) exists in the
//!   tree, shifted by the sub-row remainder for pixel-smooth scrolling. The
//!   renderer has no overflow clip, so the opaque header and footer are painted
//!   *over* the partial top/bottom rows to mask them.
//! - **Navigable:** mouse wheel, a **draggable scrollbar** (click/drag to jump
//!   anywhere — the only practical way through 1000 rows), and **keyboard**
//!   (focus the list, then PageUp/Down, Home/End, arrows).
//! - **Async via the data layer:** each visible row is a `cx.resource_blocking`
//!   keyed by index; the fetcher sleeps a deterministic-random duration on a pool
//!   thread, then returns the record. Resolved rows are cached by key.
use kurbo::{Arc, Circle, Point, Shape, Vec2};
use lumen_core::events::{Key, NamedKey};
use lumen_core::Color;
use lumen_render::canvas::Frame;
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element, TaskError};

use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use std::time::Duration;

/// Build the catalog app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

const TOTAL: usize = 1000;
const ROW_H: f64 = 46.0;
const VISIBLE: usize = 10;
const VIEW_H: f64 = ROW_H * VISIBLE as f64; // 460
const CARD_W: f64 = 480.0;
const PAD: f64 = 16.0;
const HEADER_H: f64 = 80.0; // from card top to the viewport top (masks the top)
const FOOTER_H: f64 = 50.0;
const SCROLLBAR_W: f64 = 8.0;
const CARD_H: f64 = HEADER_H + VIEW_H + FOOTER_H; // 590
const CONTENT_H: f64 = TOTAL as f64 * ROW_H;
const MAX_Y: f64 = CONTENT_H - VIEW_H;

/// A record for one star (what the async fetch resolves to). A tuple keeps it a
/// `State` type without a serde derive: (designation, distance ly, spectral-class
/// index 0..7, magnitude×100).
type Star = (String, u32, u8, u16);

/// Deterministic hash → per-row "random" data and delay are stable per index but
/// vary across rows.
fn splitmix(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn star_of(i: u64) -> Star {
    let h = splitmix(i);
    let designation = format!("GAIA DR3 {}", 1_000_000_000 + h % 8_999_999_999);
    let distance = (4 + (h >> 8) % 3600) as u32;
    let class = ((h >> 22) % 7) as u8;
    let mag = ((h >> 28) % 900) as u16;
    (designation, distance, class, mag)
}

/// Per-row mocked latency (20..240 ms), deterministic per index.
fn delay_ms(i: u64) -> u64 {
    20 + (splitmix(i ^ 0xABCD).wrapping_mul(2862933555777941757) >> 40) % 220
}

const CLASSES: [&str; 7] = ["O", "B", "A", "F", "G", "K", "M"];

fn class_color(c: u8) -> Color {
    match c {
        0 => Color::srgb8(0x9b, 0xb0, 0xff, 0xff),
        1 => Color::srgb8(0xc9, 0xd6, 0xff, 0xff),
        2 => Color::srgb8(0xf0, 0xf3, 0xff, 0xff),
        3 => Color::srgb8(0xf8, 0xf7, 0xe6, 0xff),
        4 => Color::srgb8(0xff, 0xe9, 0x9a, 0xff),
        5 => Color::srgb8(0xff, 0xc2, 0x6b, 0xff),
        _ => Color::srgb8(0xff, 0x8a, 0x5b, 0xff),
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

/// Position `el` absolutely at (`left`, `top`) with an explicit size.
fn abs(mut el: Element, top: f64, left: f64, w: f64, h: f64) -> Element {
    el.style.position = Position::Absolute;
    el.style.inset = Edges {
        top: Dim::px(top as f32),
        left: Dim::px(left as f32),
        ..Edges::AUTO
    };
    el.style.width = Dim::px(w as f32);
    el.style.height = Dim::px(h as f32);
    el
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
        f.stroke(
            &Circle::new(c, size.width / 2.0 - 1.5).to_path(0.1),
            glow,
            2.0,
        );
        f.fill_circle(c, size.width.min(size.height) / 2.0 - 3.5, color);
    })
}

/// One catalog row, fixed at `ROW_H`. Shows a spinner while loading, the record
/// once resolved.
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
            meta.style.width = Dim::px(150.0);
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
    r.style.width = Dim::pct(1.0);
    r.style.column_gap = Dim::px(12.0);
    r.style.align_items = Some(Align::Center);
    r.style.padding = Edges {
        left: Dim::px(14.0),
        right: Dim::px(14.0),
        top: Dim::px(0.0),
        bottom: Dim::px(0.0),
    };
    r
}

fn build(cx: &mut BuildCx) -> Element {
    let scroll = cx.signal("scroll", || 0.0f64);
    let y = scroll.get(cx.runtime()).clamp(0.0, MAX_Y);
    let first = (y / ROW_H).floor() as usize;
    let frac = y - first as f64 * ROW_H; // 0..ROW_H — sub-row shift for smoothness
    let last = (first + VISIBLE + 1).min(TOTAL); // +1 covers the partial bottom row

    // Visible window only; each row lazily fetches its record.
    let mut rows = Vec::with_capacity(last - first);
    let mut any_loading = false;
    let mut loaded_visible = 0usize;
    for idx in first..last {
        let star = cx.resource_blocking::<Star, TaskError, _>(
            &format!("star-{idx}"),
            idx as u64,
            move |i| {
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
    if any_loading {
        cx.animate();
    }

    // The list, shifted up by the sub-row remainder for smooth scrolling.
    let list = {
        let mut c = widgets::column(rows);
        c.style.row_gap = Dim::px(0.0);
        c.style.margin.top = Dim::px(-(frac as f32));
        c
    };
    let viewport_w = CARD_W - 2.0 * PAD - SCROLLBAR_W - 8.0;
    let mut viewport = Element {
        role: lumen_core::semantics::Role::Group,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            ..LayoutStyle::default()
        },
        on_wheel: Some(std::rc::Rc::new(move |rt, _dx, dy| {
            scroll.update(rt, |o| *o = (*o + dy).clamp(0.0, MAX_Y))
        })),
        on_key: Some(std::rc::Rc::new(move |rt, ke| {
            let step = match ke.key {
                Key::Named(NamedKey::ArrowDown) => ROW_H,
                Key::Named(NamedKey::ArrowUp) => -ROW_H,
                Key::Named(NamedKey::PageDown) => VIEW_H,
                Key::Named(NamedKey::PageUp) => -VIEW_H,
                Key::Named(NamedKey::Home) => -MAX_Y,
                Key::Named(NamedKey::End) => MAX_Y,
                _ => 0.0,
            };
            if step != 0.0 {
                scroll.update(rt, |o| *o = (*o + step).clamp(0.0, MAX_Y));
            }
        })),
        children: vec![list],
        ..Element::default()
    }
    .id("viewport")
    .focusable();
    viewport = abs(viewport, HEADER_H, PAD, viewport_w, VIEW_H);

    // Draggable scrollbar: click or drag the track to jump anywhere.
    let scrollbar = {
        let thumb_h = (VIEW_H * (VIEW_H / CONTENT_H)).max(28.0);
        let thumb_y = if MAX_Y > 0.0 {
            (y / MAX_Y) * (VIEW_H - thumb_h)
        } else {
            0.0
        };
        let mut thumb = Element::default().class("scrollbar-thumb");
        thumb.style.width = Dim::px(SCROLLBAR_W as f32);
        thumb.style.height = Dim::px(thumb_h as f32);
        thumb.style.margin.top = Dim::px(thumb_y as f32);
        let track = Element {
            role: lumen_core::semantics::Role::Group,
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..LayoutStyle::default()
            },
            // frac_y along the track maps directly to the scroll offset (jump).
            on_drag: Some(std::rc::Rc::new(move |rt, _fx, fy, _pos| {
                scroll.set(rt, fy * MAX_Y)
            })),
            children: vec![thumb],
            ..Element::default()
        }
        .class("scrollbar-track")
        .id("scrollbar");
        abs(
            track,
            HEADER_H,
            CARD_W - PAD - SCROLLBAR_W,
            SCROLLBAR_W,
            VIEW_H,
        )
    };

    // Opaque header — its background masks the partial top row painted behind it.
    let header = {
        let title = widgets::column(vec![
            txt("Gaia Stellar Survey", 20.0, 800.0).class("title"),
            txt(
                format!(
                    "Rows {}–{} of {TOTAL}",
                    first + 1,
                    (first + VISIBLE).min(TOTAL)
                ),
                12.0,
                500.0,
            )
            .class("subtitle"),
        ]);
        let status = if any_loading {
            txt(format!("{loaded_visible}/{VISIBLE} in view"), 12.0, 700.0).class("loading")
        } else {
            txt("loaded", 12.0, 700.0).class("count")
        };
        let mut bar = widgets::row(vec![title, status]);
        bar.style.align_items = Some(Align::Center);
        bar.style.justify_content = Some(Align::SpaceBetween);
        bar.style.width = Dim::pct(1.0);
        bar.style.height = Dim::pct(1.0);
        bar.style.padding = Edges::all(Dim::px(PAD as f32));
        let mut h = widgets::column(vec![bar]).id("header");
        h.background = Some(Color::srgb8(0x0c, 0x11, 0x20, 0xff)); // opaque (#surface)
        abs(h, 0.0, 0.0, CARD_W, HEADER_H)
    };

    // Opaque footer — masks the partial bottom row, and a navigation hint.
    let footer = {
        let mut hint = txt(
            "scroll · drag the bar · arrows / PgUp / PgDn / Home / End",
            11.0,
            500.0,
        )
        .class("subtitle");
        hint.style.width = Dim::pct(1.0);
        let mut f = widgets::column(vec![hint]).id("footer");
        f.background = Some(Color::srgb8(0x0c, 0x11, 0x20, 0xff));
        f.style.align_items = Some(Align::Center);
        f.style.justify_content = Some(Align::Center);
        abs(f, HEADER_H + VIEW_H, 0.0, CARD_W, FOOTER_H)
    };

    // Card: a fixed-size relative stage; children paint in order, so the header
    // and footer (after the viewport) mask its overflow; the scrollbar sits on top.
    let mut card = Element {
        role: lumen_core::semantics::Role::Group,
        style: LayoutStyle {
            position: Position::Relative,
            width: Dim::px(CARD_W as f32),
            height: Dim::px(CARD_H as f32),
            ..LayoutStyle::default()
        },
        children: vec![viewport, header, footer, scrollbar],
        ..Element::default()
    }
    .id("card");
    card.shadow = Some(Shadow::soft());

    let mut page = widgets::column(vec![card]).id("page");
    page.style.width = Dim::pct(1.0);
    page.style.height = Dim::pct(1.0);
    page.style.align_items = Some(Align::Center);
    page.style.justify_content = Some(Align::Center);
    page
}
