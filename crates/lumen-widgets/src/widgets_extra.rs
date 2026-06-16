//! The remaining 02 §10 widgets deferred from M1, completed for the 1.0 freeze
//! (T4.5): Radio, Select, Tooltip, Menu, Grid, Wrap, SplitPane, TextArea. Same
//! `Element`-constructor convention as the other widget sets.

use crate::element::{BuildCx, Element};
use lumen_core::semantics::{Action, Role, State as SemState};
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, FlexWrap, GridTrack, LayoutStyle};
use lumen_text::TextStyle;
use std::rc::Rc;

/// A radio button in the group keyed by `group`; selecting it sets the group to
/// `value`. Exactly one member of a group is checked.
pub fn radio(cx: &BuildCx, group: &str, value: usize, label: impl Into<String>) -> Element {
    let selected = cx.signal(group, || 0usize);
    let on = selected.get(cx.runtime()) == value;
    let label = label.into();
    Element {
        role: Role::Radio,
        label: label.clone(),
        focusable: true,
        actions: vec![Action::Click, Action::Focus],
        states: if on {
            vec![SemState::Checked]
        } else {
            vec![SemState::Unchecked]
        },
        style: LayoutStyle {
            padding: Edges::all(Dim::px(4.0)),
            ..LayoutStyle::default()
        },
        text: Some((
            format!("{} {label}", if on { "◉" } else { "○" }),
            TextStyle::default(),
        )),
        on_click: Some(Rc::new(move |rt| selected.set(rt, value))),
        ..Element::default()
    }
}

/// A select / combo box cycling through `options` on click. `name` keys the
/// selected index; the semantic value is the current option.
pub fn select(cx: &BuildCx, name: &str, options: &[&str]) -> Element {
    let idx = cx.signal(name, || 0usize);
    let i = idx.get(cx.runtime()).min(options.len().saturating_sub(1));
    let cur = options.get(i).copied().unwrap_or_default().to_string();
    let n = options.len();
    Element {
        role: Role::ComboBox,
        label: cur.clone(),
        value: Some(cur.clone()),
        focusable: true,
        actions: vec![Action::Click, Action::Focus, Action::SetValue],
        background: Some(Color::srgb8(0xf2, 0xf2, 0xf2, 0xff)),
        corner_radius: 4.0,
        style: LayoutStyle {
            padding: Edges::all(Dim::px(6.0)),
            min_width: Dim::px(120.0),
            ..LayoutStyle::default()
        },
        text: Some((format!("{cur} ▾"), TextStyle::default())),
        on_click: Some(Rc::new(move |rt| {
            idx.update(rt, |x| *x = (*x + 1) % n.max(1))
        })),
        ..Element::default()
    }
}

/// Wrap `target` with a tooltip whose `text` is exposed to assistive tech.
pub fn tooltip(target: Element, text: impl Into<String>) -> Element {
    let text = text.into();
    Element {
        role: Role::Group,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            ..LayoutStyle::default()
        },
        children: vec![
            target,
            Element {
                role: Role::Tooltip,
                label: text.clone(),
                text: Some((
                    text,
                    TextStyle {
                        font_size: 12.0,
                        color: Color::srgb8(0x44, 0x44, 0x44, 0xff),
                    },
                )),
                ..Element::default()
            },
        ],
        ..Element::default()
    }
}

/// A vertical menu of selectable items.
pub fn menu(items: &[&str]) -> Element {
    Element {
        role: Role::Menu,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            ..LayoutStyle::default()
        },
        children: items
            .iter()
            .map(|t| Element {
                role: Role::MenuItem,
                label: (*t).to_string(),
                focusable: true,
                actions: vec![Action::Click, Action::Focus],
                style: LayoutStyle {
                    padding: Edges::all(Dim::px(6.0)),
                    ..LayoutStyle::default()
                },
                text: Some(((*t).to_string(), TextStyle::default())),
                ..Element::default()
            })
            .collect(),
        ..Element::default()
    }
}

/// A CSS grid with `columns` equal-fraction columns.
pub fn grid(columns: usize, children: Vec<Element>) -> Element {
    Element {
        role: Role::Group,
        style: LayoutStyle {
            display: Display::Grid,
            grid_template_columns: vec![GridTrack::Fr(1.0); columns.max(1)],
            row_gap: Dim::px(4.0),
            column_gap: Dim::px(4.0),
            ..LayoutStyle::default()
        },
        children,
        ..Element::default()
    }
}

/// A flex row that wraps onto new lines.
pub fn wrap(children: Vec<Element>) -> Element {
    Element {
        role: Role::Group,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: Dim::px(4.0),
            row_gap: Dim::px(4.0),
            ..LayoutStyle::default()
        },
        children,
        ..Element::default()
    }
}

/// A two-pane horizontal split; `ratio` is the fraction given to the first pane.
pub fn split_pane(first: Element, second: Element, ratio: f32) -> Element {
    let pane = |child: Element, grow: f32| Element {
        role: Role::Group,
        style: LayoutStyle {
            flex_grow: grow,
            flex_basis: Dim::px(0.0),
            ..LayoutStyle::default()
        },
        children: vec![child],
        ..Element::default()
    };
    Element {
        role: Role::Group,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            align_items: Some(Align::Stretch),
            width: Dim::pct(1.0),
            ..LayoutStyle::default()
        },
        children: vec![
            pane(first, ratio.clamp(0.05, 0.95)),
            pane(second, (1.0 - ratio).clamp(0.05, 0.95)),
        ],
        ..Element::default()
    }
}

/// A multi-line text input. `name` keys the text; typing (including newlines)
/// appends to it.
pub fn text_area(cx: &BuildCx, name: &str, initial: &str) -> Element {
    let value = cx.signal(name, || initial.to_string());
    let v = value.get(cx.runtime());
    let shown = if v.is_empty() {
        " ".to_string()
    } else {
        v.clone()
    };
    Element {
        role: Role::TextInput,
        focusable: true,
        label: v.clone(),
        value: Some(v),
        actions: vec![Action::Focus, Action::SetValue],
        background: Some(Color::srgb8(0xf2, 0xf2, 0xf2, 0xff)),
        corner_radius: 4.0,
        style: LayoutStyle {
            padding: Edges::all(Dim::px(6.0)),
            min_width: Dim::px(160.0),
            min_height: Dim::px(72.0),
            ..LayoutStyle::default()
        },
        text: Some((shown, TextStyle::default())),
        on_text: Some(Rc::new(move |rt, t| {
            let t = t.to_string();
            value.update(rt, |s| s.push_str(&t))
        })),
        ..Element::default()
    }
    .id(name)
}

/// A modal overlay (E8.2): when `open`, `dialog` is shown centered over `base`
/// with a dimmed backdrop; otherwise just `base`.
pub fn modal(base: Element, dialog: Element, open: bool) -> Element {
    if !open {
        return base;
    }
    let backdrop = Element {
        role: Role::Group,
        background: Some(Color::srgb8(0x00, 0x00, 0x00, 0x88)),
        style: LayoutStyle {
            position: lumen_layout::Position::Absolute,
            inset: Edges::all(Dim::px(0.0)),
            display: Display::Flex,
            align_items: Some(Align::Center),
            justify_content: Some(Align::Center),
            width: Dim::pct(1.0),
            height: Dim::pct(1.0),
            ..LayoutStyle::default()
        },
        children: vec![dialog],
        ..Element::default()
    }
    .id("modal-overlay");
    crate::widgets::stack(vec![base, backdrop])
}

/// A resizable two-pane split (E8.4). Dragging within the grid sets the split
/// position; `name` keys the ratio. A visual divider marks the boundary.
pub fn pane_grid(cx: &BuildCx, name: &str, first: Element, second: Element) -> Element {
    let ratio = cx.signal(name, || 0.5f64);
    let r = ratio.get(cx.runtime());
    let pane = |child: Element, grow: f32| Element {
        role: Role::Group,
        style: LayoutStyle {
            flex_grow: grow,
            flex_basis: Dim::px(0.0),
            ..LayoutStyle::default()
        },
        children: vec![child],
        ..Element::default()
    };
    let divider = Element {
        role: Role::Generic,
        background: Some(Color::srgb8(0x88, 0x8c, 0x90, 0xff)),
        style: LayoutStyle {
            width: Dim::px(4.0),
            height: Dim::pct(1.0),
            ..LayoutStyle::default()
        },
        ..Element::default()
    }
    .id(format!("{name}-divider"));
    Element {
        role: Role::Group,
        value: Some(format!("{:.2}", r)),
        on_drag: Some(Rc::new(move |rt, frac| ratio.set(rt, frac.clamp(0.1, 0.9)))),
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            width: Dim::pct(1.0),
            height: Dim::pct(1.0),
            ..LayoutStyle::default()
        },
        children: vec![
            pane(first, r as f32),
            divider,
            pane(second, (1.0 - r) as f32),
        ],
        ..Element::default()
    }
    .id(name)
}
