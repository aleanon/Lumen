//! counter — a "tally" with a sign-reactive hero number and step controls.
//!
//! The big number and the status pill recolour with the value's sign by toggling
//! a `.lss` class (`.value.pos` / `.value.neg` / …) in `build` — dynamic theming
//! driven from the stylesheet. `just win-watch counter examples/counter/app.lss`.
use lumen_core::state::Runtime;
use lumen_widgets::element::Shadow;
use lumen_widgets::system::{MenuItem, MenuModel};
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};

/// Build the counter app.
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
        ts.font_size = 16.0;
        ts.weight = 600.0;
    }
    pad(e, 16.0, 11.0)
}

fn hrow(children: Vec<Element>, gap: f32) -> Element {
    let mut r = widgets::row(children);
    r.style.column_gap = Dim::px(gap);
    r.style.justify_content = Some(Align::Center);
    r
}

fn build(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i64);
    let v = count.get(cx.runtime());
    let (sign, label) = match v.signum() {
        1 => ("pos", "POSITIVE"),
        -1 => ("neg", "NEGATIVE"),
        _ => ("zero", "AT ZERO"),
    };

    let step = move |n: i64| move |rt: &Runtime| count.update(rt, |c| *c += n);

    // P.3c: native menu. Item ids double as `register_command` names, so a
    // native click, an accelerator chord, or the agent's `menu.invoke` all
    // run the same handler. On Linux/winit no menubar attaches (muda is
    // GTK-bound) — there the accelerators and the agent are the menu.
    cx.register_command("tally.inc", step(1));
    cx.register_command("tally.dec", step(-1));
    cx.register_command("tally.reset", move |rt| count.set(rt, 0));
    cx.set_menu(MenuModel {
        items: vec![MenuItem::submenu(
            "tally",
            "Tally",
            vec![
                MenuItem::new("tally.inc", "Increment").accel("Ctrl+I"),
                MenuItem::new("tally.dec", "Decrement").accel("Ctrl+D"),
                MenuItem::new("tally.reset", "Reset").accel("Ctrl+R"),
            ],
        )],
    });

    let mut card = widgets::column(vec![
        txt("TALLY", 13.0, 700.0).class("caption"),
        txt(format!("{v}"), 76.0, 800.0)
            .id("value")
            .class("value")
            .class(sign),
        pad(txt(label, 12.0, 700.0).class("pill").class(sign), 14.0, 5.0),
        hrow(
            vec![
                button("−10", "ghost", step(-10)).id("dec10"),
                button("−1", "ghost", step(-1)).id("dec"),
                button("+1", "accent", step(1)).id("inc"),
                button("+10", "accent", step(10)).id("inc10"),
            ],
            8.0,
        ),
        button("Reset", "reset", move |rt| count.set(rt, 0)).id("reset"),
    ])
    .id("card");
    card.style.align_items = Some(Align::Center);
    card.style.row_gap = Dim::px(16.0);
    card.style.width = Dim::px(360.0);
    card.style.padding = Edges::all(Dim::px(32.0));
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
