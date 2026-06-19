//! todos — a real add / toggle / delete list, themed from `app.lss`.
//!
//! State is a `Vec<(String, bool)>` in the store; the list is rebuilt from it.
//! Colours (accent, done-green, danger) come from the stylesheet; toggling an
//! item flips a `.done` class on its check + label.
//! `just win-watch todos examples/todos/app.lss`.
use lumen_core::semantics::{Action, Role};
use lumen_core::state::Runtime;
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};
use std::rc::Rc;

use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};

/// Build the todos app.
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

fn fill(mut e: Element) -> Element {
    e.style.flex_grow = 1.0;
    e.style.min_width = Dim::px(0.0);
    e
}

/// A clickable check box (filled when done) — no text.
fn check(done: bool, on: impl Fn(&Runtime) + 'static) -> Element {
    let mut e = Element {
        role: Role::Checkbox,
        focusable: true,
        actions: vec![Action::Click, Action::Focus],
        on_click: Some(Rc::new(on)),
        style: LayoutStyle {
            width: Dim::px(22.0),
            height: Dim::px(22.0),
            ..LayoutStyle::default()
        },
        ..Element::default()
    }
    .class("check");
    if done {
        e = e.class("done");
    }
    e
}

fn build(cx: &mut BuildCx) -> Element {
    let todos = cx.signal("todos", || {
        vec![
            ("Design the gallery".to_string(), true),
            ("Theme it from .lss".to_string(), false),
            ("Ship the examples".to_string(), false),
        ]
    });
    let draft = cx.signal("draft", String::new);
    let items = todos.get(cx.runtime());
    let total = items.len();
    let left = items.iter().filter(|(_, d)| !*d).count();

    // Header: title + a "N left" pill.
    let mut header = widgets::row(vec![
        fill(txt("Todos", 24.0, 800.0).id("title")),
        pad(
            txt(format!("{left} left"), 12.0, 700.0).class("count"),
            12.0,
            4.0,
        ),
    ]);
    header.style.align_items = Some(Align::Center);

    // Input row: field + Add.
    let field = fill(widgets::text_field_basic(cx, "draft", "").id("draft"));
    let mut add = widgets::button("Add", move |rt| {
        let t = draft.get(rt).trim().to_string();
        if !t.is_empty() {
            todos.update(rt, move |v| v.push((t.clone(), false)));
            draft.set(rt, String::new());
        }
    })
    .class("add")
    .id("add");
    if let Some(ts) = add.text_style_mut() {
        ts.font_size = 15.0;
        ts.weight = 600.0;
    }
    let mut input = widgets::row(vec![field, pad(add, 18.0, 10.0)]);
    input.style.column_gap = Dim::px(10.0);
    input.style.align_items = Some(Align::Center);

    // Item rows.
    let rows: Vec<Element> = items
        .iter()
        .enumerate()
        .map(|(i, (text, done))| {
            let done = *done;
            let tog = check(done, move |rt| {
                todos.update(rt, move |v| {
                    if let Some(it) = v.get_mut(i) {
                        it.1 = !it.1;
                    }
                })
            })
            .id(format!("check-{i}"));
            let mut lbl = fill(txt(text.clone(), 15.0, 500.0).class("label"));
            if done {
                lbl = lbl.class("done");
            }
            let mut del = widgets::button("Del", move |rt| {
                todos.update(rt, move |v| {
                    if i < v.len() {
                        v.remove(i);
                    }
                })
            })
            .class("del")
            .id(format!("del-{i}"));
            if let Some(ts) = del.text_style_mut() {
                ts.font_size = 13.0;
                ts.weight = 600.0;
            }
            let mut r = widgets::row(vec![tog, lbl, pad(del, 8.0, 6.0)]);
            r.style.column_gap = Dim::px(12.0);
            r.style.align_items = Some(Align::Center);
            r.style.width = Dim::pct(1.0);
            r
        })
        .collect();
    let mut list = widgets::column(rows);
    list.style.row_gap = Dim::px(10.0);
    list.style.width = Dim::pct(1.0);

    let footer = txt(format!("{} of {total} done", total - left), 13.0, 500.0).class("footer");

    let mut card = widgets::column(vec![header, input, list, footer]).id("card");
    card.style.width = Dim::px(420.0);
    card.style.padding = Edges::all(Dim::px(28.0));
    card.style.row_gap = Dim::px(18.0);
    card.style.align_items = Some(Align::Stretch);
    card.shadow = Some(Shadow::soft());

    Element {
        role: Role::Group,
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
