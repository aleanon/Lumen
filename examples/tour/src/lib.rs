//! tour — a stepped walkthrough: kicker + hero number, title/body, progress
//! dots, and Back/Next (Next becomes Finish). Themed from `app.lss`.
use lumen_core::state::Runtime;
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};

/// Build the tour app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

const PAGES: &[(&str, &str)] = &[
    (
        "Welcome",
        "Lumen builds native UIs from text. Let's take a quick tour.",
    ),
    (
        "Widgets",
        "A composable library: text, buttons, inputs, lists, canvas.",
    ),
    (
        "Layout",
        "Flexbox and grid via Taffy — logical-px and DPI-correct.",
    ),
    (
        "Styling",
        ".lss stylesheets with tokens and classes; hot-reloadable.",
    ),
    (
        "Ship it",
        "Run on desktop, web, and mobile from one codebase.",
    ),
];

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

fn button(label: &str, kind: &str, on: impl Fn(&Runtime) + 'static) -> Element {
    let mut e = widgets::button(label, on).class("btn").class(kind);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = 15.0;
        ts.weight = 600.0;
    }
    pad(e, 22.0, 11.0)
}

fn dot(on: bool) -> Element {
    let mut e = Element {
        style: LayoutStyle {
            width: Dim::px(9.0),
            height: Dim::px(9.0),
            ..LayoutStyle::default()
        },
        ..Element::default()
    }
    .class("dot");
    if on {
        e = e.class("on");
    }
    e
}

fn build(cx: &mut BuildCx) -> Element {
    let step = cx.signal("step", || 0i64);
    let n = PAGES.len() as i64;
    let i = step.get(cx.runtime()).clamp(0, n - 1);
    let (title, body) = PAGES[i as usize];
    let last = i == n - 1;

    let hero = {
        let mut h = pad(
            txt(format!("{}", i + 1), 40.0, 800.0).class("hero-num"),
            0.0,
            0.0,
        );
        h.style.width = Dim::px(84.0);
        h.style.height = Dim::px(84.0);
        h.style.align_items = Some(Align::Center);
        h.style.justify_content = Some(Align::Center);
        h.class("hero")
    };

    let dots = {
        let mut r = widgets::row((0..n).map(|d| dot(d == i)).collect::<Vec<_>>());
        r.style.column_gap = Dim::px(7.0);
        r.style.justify_content = Some(Align::Center);
        r
    };

    let nav = {
        let back = button("Back", "back", move |rt| {
            step.update(rt, |s| *s = (*s - 1).max(0))
        })
        .id("back");
        let next = button(if last { "Finish" } else { "Next" }, "next", move |rt| {
            step.update(rt, move |s| *s = if *s + 1 >= n { 0 } else { *s + 1 })
        })
        .id("next");
        let mut r = widgets::row(vec![back, next]);
        r.style.column_gap = Dim::px(10.0);
        r.style.justify_content = Some(Align::Center);
        r
    };

    let mut card = widgets::column(vec![
        txt(format!("STEP {} OF {n}", i + 1), 12.0, 700.0).class("kicker"),
        hero,
        txt(title, 26.0, 800.0).class("title").id("title"),
        txt(body, 15.0, 400.0).class("body"),
        dots,
        nav,
    ])
    .id("card");
    card.style.width = Dim::px(420.0);
    card.style.padding = Edges::all(Dim::px(34.0));
    card.style.row_gap = Dim::px(16.0);
    card.style.align_items = Some(Align::Center);
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
