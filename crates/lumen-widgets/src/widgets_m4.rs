//! M4 depth widgets (02 §depth): DataGrid, Tree, bar Chart, RichText(+Editor).
//! `Element` constructors like the other widget sets; stateful ones own a signal
//! keyed by `name`.

use crate::element::{BuildCx, Element};
use lumen_core::semantics::{Action, Role, ScrollInfo, State as SemState};
use lumen_core::Color;
use lumen_layout::{Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use lumen_text::TextStyle;
use std::collections::HashSet;
use std::rc::Rc;

const OVERSCAN: usize = 2;

fn cell(text: String, role: Role) -> Element {
    Element {
        role,
        label: text.clone(),
        style: LayoutStyle {
            flex_grow: 1.0,
            flex_basis: Dim::px(0.0),
            padding: Edges::all(Dim::px(4.0)),
            ..LayoutStyle::default()
        },
        text: Some((text, TextStyle::default())),
        ..Element::default()
    }
}

/// A virtualized data grid: a header plus a windowed body that materializes only
/// the visible rows, so cost is independent of `row_count` (supports 1M+ rows).
/// `name` keys the vertical scroll offset.
pub fn data_grid(
    cx: &BuildCx,
    name: &str,
    columns: &[&str],
    row_count: usize,
    row_height: f64,
    viewport_h: f64,
    cell_text: impl Fn(usize, usize) -> String,
) -> Element {
    let offset = cx.signal(name, || 0.0f64);
    let y = offset.get(cx.runtime());
    let ncols = columns.len();

    let header = Element {
        role: Role::Row,
        background: Some(Color::srgb8(0xe8, 0xea, 0xed, 0xff)),
        style: row_style(),
        children: columns
            .iter()
            .map(|c| cell(c.to_string(), Role::ColumnHeader))
            .collect(),
        ..Element::default()
    };

    let first = ((y / row_height).floor() as usize).saturating_sub(OVERSCAN);
    let per_view = (viewport_h / row_height).ceil() as usize;
    let last = (first + per_view + OVERSCAN * 2).min(row_count);

    let rows: Vec<Element> = (first..last)
        .map(|r| {
            let top = (r as f64 * row_height) - y;
            let cells = (0..ncols)
                .map(|c| cell(cell_text(r, c), Role::Cell))
                .collect();
            Element {
                role: Role::Row,
                style: LayoutStyle {
                    position: Position::Absolute,
                    inset: Edges {
                        left: Dim::px(0.0),
                        top: Dim::px(top as f32),
                        ..Edges::AUTO
                    },
                    width: Dim::pct(1.0),
                    height: Dim::px(row_height as f32),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    ..LayoutStyle::default()
                },
                children: cells,
                ..Element::default()
            }
        })
        .collect();

    let max_y = (row_count as f64 * row_height - viewport_h).max(0.0);
    let body = Element {
        role: Role::Group,
        scroll: Some(ScrollInfo {
            x: 0.0,
            y,
            max_x: 0.0,
            max_y,
        }),
        style: LayoutStyle {
            position: Position::Relative,
            width: Dim::pct(1.0),
            height: Dim::px(viewport_h as f32),
            ..LayoutStyle::default()
        },
        on_wheel: Some(Rc::new(move |rt, dy| {
            offset.update(rt, |o| *o = (*o + dy).clamp(0.0, max_y))
        })),
        children: rows,
        ..Element::default()
    };

    Element {
        role: Role::Table,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Dim::pct(1.0),
            ..LayoutStyle::default()
        },
        children: vec![header, body],
        ..Element::default()
    }
}

fn row_style() -> LayoutStyle {
    LayoutStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        width: Dim::pct(1.0),
        ..LayoutStyle::default()
    }
}

/// One row of a [`tree`]: `depth` is its indent level; `has_children` enables an
/// expand toggle.
pub struct TreeRow<'a> {
    /// Stable id (also the expand-set key).
    pub id: &'a str,
    /// Display label.
    pub label: &'a str,
    /// Indentation depth (0 = root).
    pub depth: usize,
    /// Whether this node has children to expand.
    pub has_children: bool,
}

/// An expand/collapse tree. `name` keys the set of expanded ids; collapsing a
/// node hides its descendants. Clicking a parent toggles it.
pub fn tree(cx: &BuildCx, name: &str, rows: &[TreeRow]) -> Element {
    let expanded = cx.signal(name, HashSet::<String>::new);
    let exp = expanded.get(cx.runtime());

    let mut cutoff: Option<usize> = None;
    let mut children = Vec::new();
    for row in rows {
        if let Some(c) = cutoff {
            if row.depth > c {
                continue;
            }
            cutoff = None;
        }
        let is_open = exp.contains(row.id);
        let marker = if row.has_children {
            if is_open {
                "▾ "
            } else {
                "▸ "
            }
        } else {
            "• "
        };
        let label = format!("{marker}{}", row.label);
        let id = row.id.to_string();
        let toggleable = row.has_children;
        let states = if !row.has_children {
            vec![]
        } else if is_open {
            vec![SemState::Expanded]
        } else {
            vec![SemState::Collapsed]
        };
        children.push(Element {
            role: Role::TreeItem,
            label: label.clone(),
            focusable: true,
            actions: vec![Action::Click, Action::Focus],
            states,
            style: LayoutStyle {
                padding: Edges {
                    left: Dim::px((row.depth as f32) * 16.0 + 4.0),
                    ..Edges::all(Dim::px(4.0))
                },
                ..LayoutStyle::default()
            },
            text: Some((label, TextStyle::default())),
            on_click: toggleable.then(|| {
                Rc::new(move |rt: &lumen_core::Runtime| {
                    expanded.update(rt, |set| {
                        if !set.remove(&id) {
                            set.insert(id.clone());
                        }
                    })
                }) as crate::element::Handler
            }),
            ..Element::default()
        });
        // A collapsed parent hides everything deeper than it.
        if row.has_children && !is_open {
            cutoff = Some(row.depth);
        }
    }

    Element {
        role: Role::Tree,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            ..LayoutStyle::default()
        },
        children,
        ..Element::default()
    }
}

/// A simple bar chart: one bar per value, heights proportional to the max.
/// `name` is the accessible label; the value is the bar count.
pub fn bar_chart(values: &[f64], width: f64, height: f64) -> Element {
    let max = values.iter().cloned().fold(f64::MIN, f64::max).max(1e-9);
    let bars = values
        .iter()
        .map(|v| {
            let frac = (v / max).clamp(0.0, 1.0);
            Element {
                role: Role::Generic,
                background: Some(Color::srgb8(0x1a, 0x73, 0xe8, 0xff)),
                style: LayoutStyle {
                    flex_grow: 1.0,
                    flex_basis: Dim::px(0.0),
                    height: Dim::px((frac * height) as f32),
                    align_self: Some(lumen_layout::Align::End),
                    ..LayoutStyle::default()
                },
                ..Element::default()
            }
        })
        .collect();
    Element {
        role: Role::Group,
        label: format!("bar chart, {} values", values.len()),
        value: Some(format!("{}", values.len())),
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            align_items: Some(lumen_layout::Align::End),
            column_gap: Dim::px(2.0),
            width: Dim::px(width as f32),
            height: Dim::px(height as f32),
            ..LayoutStyle::default()
        },
        children: bars,
        ..Element::default()
    }
}

/// One styled run of [`rich_text`].
pub struct Run<'a> {
    /// Text content.
    pub text: &'a str,
    /// Run colour.
    pub color: Color,
    /// Font size (px).
    pub size: f32,
}

/// A paragraph of differently-styled runs laid out in a row.
pub fn rich_text(runs: &[Run]) -> Element {
    let children = runs
        .iter()
        .map(|r| Element {
            role: Role::Text,
            label: r.text.to_string(),
            text: Some((
                r.text.to_string(),
                TextStyle {
                    font_size: r.size,
                    color: r.color,
                },
            )),
            ..Element::default()
        })
        .collect();
    Element {
        role: Role::Group,
        label: runs.iter().map(|r| r.text).collect::<String>(),
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            column_gap: Dim::px(2.0),
            ..LayoutStyle::default()
        },
        children,
        ..Element::default()
    }
}

/// An editable rich-text field: stores markdown-lite source (`*emphasis*`) in a
/// signal and renders it as styled [`rich_text`] runs. Typing appends to the
/// source; the semantic `value` is the raw source.
pub fn rich_text_editor(cx: &BuildCx, name: &str, initial: &str) -> Element {
    let value = cx.signal(name, || initial.to_string());
    let src = value.get(cx.runtime());

    let runs = parse_runs(&src);
    let run_refs: Vec<Run> = runs
        .iter()
        .map(|(t, em)| Run {
            text: t,
            color: if *em {
                Color::srgb8(0xc0, 0x39, 0x2b, 0xff)
            } else {
                Color::BLACK
            },
            size: 16.0,
        })
        .collect();
    let mut display = rich_text(&run_refs);
    display.role = Role::TextInput;
    display.focusable = true;
    display.value = Some(src.clone());
    display.label = src.clone();
    display.actions = vec![Action::Focus, Action::SetValue];
    display.background = Some(Color::srgb8(0xf2, 0xf2, 0xf2, 0xff));
    display.corner_radius = 4.0;
    display.style.padding = Edges::all(Dim::px(6.0));
    display.style.min_width = Dim::px(160.0);
    display.on_text = Some(Rc::new(move |rt, t| {
        let t = t.to_string();
        value.update(rt, |s| s.push_str(&t))
    }));
    // Focus tracking is keyed by StableId, so the field needs one.
    display.id(name)
}

/// Split markdown-lite source into `(text, emphasised)` runs on `*…*`.
fn parse_runs(src: &str) -> Vec<(String, bool)> {
    let mut runs = Vec::new();
    let mut cur = String::new();
    let mut em = false;
    for ch in src.chars() {
        if ch == '*' {
            if !cur.is_empty() {
                runs.push((std::mem::take(&mut cur), em));
            }
            em = !em;
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        runs.push((cur, em));
    }
    if runs.is_empty() {
        runs.push((" ".to_string(), false));
    }
    runs
}
