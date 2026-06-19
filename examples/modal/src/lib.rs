//! modal — a dialog over a dimmed scrim, toggled by a signal. Themed from
//! `app.lss`; the scrim is a full-window absolute overlay painted last.
use lumen_core::state::Runtime;
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle, Position};

/// Build the modal app.
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
        ts.font_size = 15.0;
        ts.weight = 600.0;
    }
    pad(e, 20.0, 11.0)
}

fn build(cx: &mut BuildCx) -> Element {
    let open = cx.signal("open", || false);
    let is_open = open.get(cx.runtime());

    let mut base = widgets::column(vec![
        txt("Modal", 26.0, 800.0).class("title"),
        txt("A dialog presented over a dimmed scrim.", 15.0, 400.0).class("body"),
        button("Open dialog", "accent", move |rt| open.set(rt, true)).id("open"),
    ])
    .id("card");
    base.style.align_items = Some(Align::Center);
    base.style.row_gap = Dim::px(16.0);
    base.style.padding = Edges::all(Dim::px(34.0));
    base.shadow = Some(Shadow::soft());

    let mut children = vec![base];

    if is_open {
        let mut dialog = widgets::column(vec![
            txt("Delete file?", 20.0, 800.0).class("dialog-title"),
            txt("This can't be undone. Are you sure?", 14.0, 400.0).class("dialog-body"),
            {
                let mut r = widgets::row(vec![
                    button("Cancel", "ghost", move |rt| open.set(rt, false)).id("cancel"),
                    button("Delete", "accent", move |rt| open.set(rt, false)).id("confirm"),
                ]);
                r.style.column_gap = Dim::px(10.0);
                r
            },
        ])
        .id("dialog")
        .class("dialog");
        dialog.style.align_items = Some(Align::Center);
        dialog.style.row_gap = Dim::px(14.0);
        dialog.style.width = Dim::px(340.0);
        dialog.style.padding = Edges::all(Dim::px(26.0));
        dialog.shadow = Some(Shadow::soft());

        let overlay = Element {
            role: lumen_core::semantics::Role::Group,
            style: LayoutStyle {
                position: Position::Absolute,
                inset: Edges::all(Dim::px(0.0)),
                display: Display::Flex,
                align_items: Some(Align::Center),
                justify_content: Some(Align::Center),
                ..LayoutStyle::default()
            },
            children: vec![dialog],
            ..Element::default()
        }
        .class("scrim")
        .id("scrim");
        children.push(overlay);
    }

    Element {
        role: lumen_core::semantics::Role::Group,
        style: LayoutStyle {
            position: Position::Relative,
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Dim::pct(1.0),
            height: Dim::pct(1.0),
            align_items: Some(Align::Center),
            justify_content: Some(Align::Center),
            ..LayoutStyle::default()
        },
        children,
        ..Element::default()
    }
    .id("page")
}
