//! events — an "event inspector": the latest interaction as a hero readout, a
//! row of typed trigger buttons, and a rolling log. Themed from `app.lss`.
use lumen_core::state::Runtime;
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};

/// Build the events app.
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
        ts.font_size = 14.0;
        ts.weight = 600.0;
    }
    pad(e, 16.0, 10.0)
}

fn build(cx: &mut BuildCx) -> Element {
    let log = cx.signal("log", || vec!["app started".to_string()]);
    let lines = log.get(cx.runtime());
    let last = lines.last().cloned().unwrap_or_default();
    let push = move |s: &'static str| {
        move |rt: &Runtime| {
            log.update(rt, move |v| {
                v.push(s.to_string());
                if v.len() > 6 {
                    v.remove(0);
                }
            })
        }
    };

    let header = {
        let mut c = widgets::column(vec![
            txt("Event Inspector", 24.0, 800.0).class("title"),
            txt(
                "The last interaction, observed via the semantic tree.",
                14.0,
                400.0,
            )
            .class("subtitle"),
        ]);
        c.style.row_gap = Dim::px(4.0);
        c
    };

    let hero = pad(txt(last, 26.0, 800.0).class("event").id("log"), 0.0, 6.0);

    let buttons = {
        let mut r = widgets::row(vec![
            button("Primary", "primary", push("primary button tapped")).id("primary"),
            button("Secondary", "secondary", push("secondary tapped")).id("secondary"),
            button("Danger", "danger", push("danger action fired")).id("danger"),
        ]);
        r.style.column_gap = Dim::px(10.0);
        r
    };

    let mut log_rows: Vec<Element> = lines
        .iter()
        .rev()
        .map(|l| txt(format!("• {l}"), 13.0, 500.0).class("logline"))
        .collect();
    let mut well = widgets::column(std::mem::take(&mut log_rows));
    well.style.row_gap = Dim::px(6.0);
    well.style.align_items = Some(Align::Start);
    well.style.width = Dim::pct(1.0);
    let well = {
        let inner = pad(well, 16.0, 14.0).class("well");
        let mut s = widgets::column(vec![txt("RECENT", 12.0, 700.0).class("section"), inner]);
        s.style.row_gap = Dim::px(8.0);
        s.style.width = Dim::pct(1.0);
        s
    };

    let mut card = widgets::column(vec![header, hero, buttons, well]).id("card");
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
