//! M1 widget additions (02 §10), most importantly the windowing `VirtualList`.
//!
//! These are `Element` constructors like the M0 primitives; stateful ones own a
//! signal keyed by `name`. The remaining 02 §10 M1 widgets (RichText, Grid,
//! Wrap, Align, SplitPane, TextArea, Select, Tooltip, Popover, Menu) follow the
//! same constructor pattern.

use crate::element::{BuildCx, Element};
use lumen_core::semantics::{Action, Role, ScrollInfo, State as SemState};
use lumen_core::Color;
use lumen_layout::{Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use std::rc::Rc;

/// Flexible empty space that grows to fill its container.
pub fn spacer() -> Element {
    Element {
        role: Role::Generic,
        elide_semantics: true,
        style: LayoutStyle {
            flex_grow: 1.0,
            ..LayoutStyle::default()
        },
        ..Element::default()
    }
}

/// A horizontal divider line.
pub fn divider() -> Element {
    Element {
        role: Role::Generic,
        background: Some(Color::srgb8(0xd8, 0xdd, 0xe3, 0xff)),
        style: LayoutStyle {
            height: Dim::px(1.0),
            width: Dim::pct(1.0),
            ..LayoutStyle::default()
        },
        ..Element::default()
    }
}

/// Wrap `child` in uniform padding (px).
pub fn padding(px: f32, child: Element) -> Element {
    Element {
        role: Role::Generic,
        elide_semantics: true,
        style: LayoutStyle {
            padding: Edges::all(Dim::px(px)),
            ..LayoutStyle::default()
        },
        children: vec![child],
        ..Element::default()
    }
}

/// An icon placeholder (a small filled square; glyph icons land with RichText).
pub fn icon(label: &str) -> Element {
    Element {
        role: Role::Image,
        label: label.to_string(),
        background: Some(Color::srgb8(0x55, 0x5a, 0x61, 0xff)),
        corner_radius: 2.0,
        style: LayoutStyle {
            width: Dim::px(16.0),
            height: Dim::px(16.0),
            ..LayoutStyle::default()
        },
        ..Element::default()
    }
}

/// A toggle switch with its own boolean state (`name`).
pub fn switch(cx: &BuildCx, name: &str, label: impl Into<String>) -> Element {
    let label = label.into();
    let on = cx.signal(name, || false);
    let is = on.get(cx.runtime());
    let track = Element {
        background: Some(if is {
            Color::srgb8(0x1a, 0x73, 0xe8, 0xff)
        } else {
            Color::srgb8(0xcc, 0xcc, 0xcc, 0xff)
        }),
        corner_radius: 10.0,
        style: LayoutStyle {
            width: Dim::px(36.0),
            height: Dim::px(20.0),
            ..LayoutStyle::default()
        },
        ..Element::default()
    };
    Element {
        role: Role::Switch,
        label: label.clone(),
        focusable: true,
        actions: vec![Action::Click, Action::Focus],
        states: vec![if is {
            SemState::Checked
        } else {
            SemState::Unchecked
        }],
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            column_gap: Dim::px(6.0),
            ..LayoutStyle::default()
        },
        on_click: Some(Rc::new(move |rt| on.update(rt, |v| *v = !*v))),
        children: vec![track, Element::text(label)],
        ..Element::default()
    }
}

/// A numeric stepper (`-`/value/`+`) with its own integer state (`name`).
pub fn stepper(cx: &BuildCx, name: &str, min: i64, max: i64) -> Element {
    let value = cx.signal(name, || min);
    let v = value.get(cx.runtime());
    let dec = crate::widgets::button("-", move |rt| value.update(rt, |x| *x = (*x - 1).max(min)))
        .id("dec");
    let inc = crate::widgets::button("+", move |rt| value.update(rt, |x| *x = (*x + 1).min(max)))
        .id("inc");
    Element {
        role: Role::Group,
        label: format!("{v}"),
        value: Some(format!("{v}")),
        actions: vec![Action::Increment, Action::Decrement],
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            column_gap: Dim::px(8.0),
            ..LayoutStyle::default()
        },
        children: vec![dec, Element::text(format!("{v}")).id("value"), inc],
        ..Element::default()
    }
}

/// A tab bar with its own selected-index state (`name`).
pub fn tabs(cx: &BuildCx, name: &str, labels: &[&str]) -> Element {
    let selected = cx.signal(name, || 0usize);
    let cur = selected.get(cx.runtime());
    let tabs: Vec<Element> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let on = i == cur;
            Element {
                role: Role::Tab,
                label: (*label).to_string(),
                focusable: true,
                actions: vec![Action::Click, Action::Focus],
                states: if on { vec![SemState::Selected] } else { vec![] },
                background: Some(if on {
                    Color::srgb8(0x1a, 0x73, 0xe8, 0xff)
                } else {
                    Color::srgb8(0xee, 0xf0, 0xf3, 0xff)
                }),
                corner_radius: 4.0,
                style: LayoutStyle {
                    padding: Edges::all(Dim::px(6.0)),
                    ..LayoutStyle::default()
                },
                content: crate::NodeContent::Text(
                    (*label).to_string(),
                    lumen_text::TextStyle {
                        font_size: 14.0,
                        weight: 400.0,
                        color: if on { Color::WHITE } else { Color::BLACK },
                        line_height: None,
                        letter_spacing: 0.0,
                        family: None,
                    },
                ),
                on_click: Some(Rc::new(move |rt| selected.set(rt, i))),
                ..Element::default()
            }
        })
        .collect();
    Element {
        role: Role::TabList,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            column_gap: Dim::px(4.0),
            ..LayoutStyle::default()
        },
        children: tabs,
        ..Element::default()
    }
}

/// A windowing list (02 §10): materializes only the visible items plus overscan,
/// regardless of `item_count`. State (`name`) is the scroll offset.
pub fn virtual_list(
    cx: &BuildCx,
    name: &str,
    item_count: usize,
    item_height: f64,
    viewport_h: f64,
    render: impl Fn(usize) -> Element,
) -> Element {
    const OVERSCAN: usize = 2;
    let offset = cx.signal(name, || 0.0f64);
    let y = offset.get(cx.runtime());

    let first = ((y / item_height).floor() as usize).saturating_sub(OVERSCAN);
    let per_view = (viewport_h / item_height).ceil() as usize;
    let last = (first + per_view + OVERSCAN * 2).min(item_count);

    let children: Vec<Element> = (first..last)
        .map(|i| {
            let top = (i as f64 * item_height) - y;
            let mut el = render(i);
            el.style.position = Position::Absolute;
            el.style.inset = Edges {
                left: Dim::px(0.0),
                top: Dim::px(top as f32),
                ..Edges::AUTO
            };
            el.style.height = Dim::px(item_height as f32);
            el
        })
        .collect();

    let max_y = (item_count as f64 * item_height - viewport_h).max(0.0);
    Element {
        role: Role::List,
        scroll: Some(ScrollInfo {
            x: 0.0,
            y,
            max_x: 0.0,
            max_y,
        }),
        actions: vec![Action::ScrollIntoView],
        style: LayoutStyle {
            position: Position::Relative,
            width: Dim::pct(1.0),
            height: Dim::px(viewport_h as f32),
            ..LayoutStyle::default()
        },
        on_wheel: Some(Rc::new(move |rt, dy| {
            offset.update(rt, |o| *o = (*o + dy).clamp(0.0, max_y))
        })),
        children,
        ..Element::default()
    }
}
