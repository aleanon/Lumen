//! M4 depth widgets (02 §depth): DataGrid, Tree, bar Chart, RichText(+Editor).
//! `Element` constructors like the other widget sets; stateful ones own a signal
//! keyed by `name`.

use crate::element::{BuildCx, Element};
use crate::widget::impl_common;
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
        content: crate::NodeContent::Text(text, TextStyle::default()),
        ..Element::default()
    }
}

/// [`DataGrid`] — a virtualized table (header + windowed rows); scroll
/// offset under `name` (typed form of [`data_grid`]).
pub struct DataGrid {
    el: Element,
}

impl DataGrid {
    /// A virtualized data grid: a header plus a windowed body that materializes only
    /// the visible rows, so cost is independent of `row_count` (supports 1M+ rows).
    /// `name` keys the vertical scroll offset.
    pub fn new(
        cx: &BuildCx,
        name: &str,
        columns: &[&str],
        row_count: usize,
        row_height: f64,
        viewport_h: f64,
        cell_text: impl Fn(usize, usize) -> String,
    ) -> DataGrid {
        let el = {
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
                on_wheel: Some(Rc::new(move |rt, _dx, dy, _mods| {
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
        };
        DataGrid { el }
    }
}

impl_common!(DataGrid);

/// A virtualized data grid: a header plus a windowed body that materializes only
/// the visible rows, so cost is independent of `row_count` (supports 1M+ rows).
/// `name` keys the vertical scroll offset.
/// *(Thin shim over [`DataGrid`] — the typed form is preferred.)*
pub fn data_grid(
    cx: &BuildCx,
    name: &str,
    columns: &[&str],
    row_count: usize,
    row_height: f64,
    viewport_h: f64,
    cell_text: impl Fn(usize, usize) -> String,
) -> Element {
    DataGrid::new(
        cx, name, columns, row_count, row_height, viewport_h, cell_text,
    )
    .into()
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

/// [`Tree`] — an expand/collapse tree; expanded set under `name` (typed
/// form of [`tree`]).
pub struct Tree {
    el: Element,
}

impl Tree {
    /// An expand/collapse tree. `name` keys the set of expanded ids; collapsing a
    /// node hides its descendants. Clicking a parent toggles it.
    pub fn new(cx: &BuildCx, name: &str, rows: &[TreeRow]) -> Tree {
        let el = {
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
                    content: crate::NodeContent::Text(label, TextStyle::default()),
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
        };
        Tree { el }
    }
}

impl_common!(Tree);

/// An expand/collapse tree. `name` keys the set of expanded ids; collapsing a
/// node hides its descendants. Clicking a parent toggles it.
/// *(Thin shim over [`Tree`] — the typed form is preferred.)*
pub fn tree(cx: &BuildCx, name: &str, rows: &[TreeRow]) -> Element {
    Tree::new(cx, name, rows).into()
}

/// [`BarChart`] — vertical bars for `values` (typed form of
/// [`bar_chart`]).
pub struct BarChart {
    el: Element,
}

impl BarChart {
    /// A simple bar chart: one bar per value, heights proportional to the max.
    /// `name` is the accessible label; the value is the bar count.
    pub fn new(values: &[f64], width: f64, height: f64) -> BarChart {
        let el = {
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
        };
        BarChart { el }
    }
}

impl_common!(BarChart);

/// A simple bar chart: one bar per value, heights proportional to the max.
/// `name` is the accessible label; the value is the bar count.
/// *(Thin shim over [`BarChart`] — the typed form is preferred.)*
pub fn bar_chart(values: &[f64], width: f64, height: f64) -> Element {
    BarChart::new(values, width, height).into()
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

/// [`RichText`] — a row of differently-styled text runs (typed form of
/// [`rich_text`]).
pub struct RichText {
    el: Element,
}

impl RichText {
    /// A paragraph of differently-styled runs laid out in a row.
    pub fn new(runs: &[Run]) -> RichText {
        let el = {
            let children = runs
                .iter()
                .map(|r| Element {
                    role: Role::Text,
                    label: r.text.to_string(),
                    content: crate::NodeContent::Text(
                        r.text.to_string(),
                        TextStyle {
                            font_size: r.size,
                            weight: 400.0,
                            color: r.color,
                            line_height: None,
                            letter_spacing: 0.0,
                            family: None,
                        },
                    ),
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
        };
        RichText { el }
    }
}

impl_common!(RichText);

/// A paragraph of differently-styled runs laid out in a row.
/// *(Thin shim over [`RichText`] — the typed form is preferred.)*
pub fn rich_text(runs: &[Run]) -> Element {
    RichText::new(runs).into()
}

/// [`RichTextEditor`] — a markdown-lite source editor with a live parsed
/// preview; state under `name` (typed form of [`rich_text_editor`]).
pub struct RichTextEditor {
    el: Element,
}

impl RichTextEditor {
    /// M.4: the rich-text editor — the `RichDoc` model edited at the SOURCE
    /// level with the full [`lumen_text::TextEditor`] caret/selection/clipboard/
    /// undo machinery (same engine as `TextField`), plus a live parsed preview.
    /// State: `{name}` holds the `TextEditor`; `{name}.text` mirrors the source
    /// for plain reads. The semantic value is the source; the preview subtree
    /// carries the rendered document (links/lists/images per [`crate::richdoc`]).
    pub fn new(cx: &BuildCx, name: &str, initial: &str) -> RichTextEditor {
        let el = {
            use lumen_text::TextEditor;
            let editor = cx.signal(name, || TextEditor::new(initial));
            let mirror = cx.signal(&format!("{name}.text"), || initial.to_string());
            let ed = editor.get(cx.runtime());
            let src = ed.text().to_string();
            let shown = if src.is_empty() {
                " ".to_string()
            } else {
                src.clone()
            };

            // The source pane: real caret + selection on the markdown-lite source.
            let source_pane = Element {
                role: Role::TextInput,
                focusable: true,
                label: src.clone(),
                value: Some(src.clone()),
                actions: vec![Action::Focus, Action::SetValue],
                background: Some(Color::srgb8(0xf2, 0xf2, 0xf2, 0xff)),
                corner_radius: 4.0,
                style: LayoutStyle {
                    padding: Edges::all(Dim::px(6.0)),
                    min_width: Dim::px(220.0),
                    min_height: Dim::px(56.0),
                    width: Dim::px(300.0),
                    ..LayoutStyle::default()
                },
                content: crate::NodeContent::Text(shown, lumen_text::TextStyle::default()),
                caret_byte: Some(ed.cursor()),
                selection: ed.has_selection().then(|| ed.selection()),
                on_text: Some(Rc::new(move |rt, t| {
                    editor.update(rt, |e| e.insert(t));
                    mirror.set(rt, editor.get(rt).text().to_string());
                })),
                on_caret_set: Some(Rc::new(move |rt, byte, extend| {
                    editor.update(rt, |e| e.place(byte, extend));
                })),
                on_key: Some(Rc::new(move |rt, ke| {
                    crate::text_input::edit_key(rt, ke, editor, mirror, true);
                })),
                ..Element::default()
            }
            .id(name);

            // The live preview: the parsed RichDoc (lists, links, images).
            let doc = crate::richdoc::RichDoc::parse(&src);
            let mut preview = doc.render(|_, _| {});
            preview = preview.id(format!("{name}-preview"));

            let mut col = crate::widgets::column(vec![source_pane, preview]);
            col.style.row_gap = Dim::px(8.0);
            col
        };
        RichTextEditor { el }
    }
}

impl_common!(RichTextEditor);

/// M.4: the rich-text editor — the `RichDoc` model edited at the SOURCE
/// level with the full [`lumen_text::TextEditor`] caret/selection/clipboard/
/// undo machinery (same engine as `TextField`), plus a live parsed preview.
/// State: `{name}` holds the `TextEditor`; `{name}.text` mirrors the source
/// for plain reads. The semantic value is the source; the preview subtree
/// carries the rendered document (links/lists/images per [`crate::richdoc`]).
/// *(Thin shim over [`RichTextEditor`] — the typed form is preferred.)*
pub fn rich_text_editor(cx: &BuildCx, name: &str, initial: &str) -> Element {
    RichTextEditor::new(cx, name, initial).into()
}

/// [`FindReplaceBar`] — find/replace over a [`RichTextEditor`]'s source;
/// inputs under `name` (typed form of [`find_replace_bar`]).
pub struct FindReplaceBar {
    el: Element,
}

impl FindReplaceBar {
    /// M.4: a find/replace bar operating on a [`rich_text_editor`]'s source.
    /// `{name}.find` / `{name}.replace` hold the inputs; the count label shows
    /// live match counts and the button rewrites every occurrence (caret resets
    /// to the end; the editor's undo history keeps the previous text).
    pub fn new(cx: &BuildCx, name: &str, editor_name: &str) -> FindReplaceBar {
        let el = {
            use lumen_text::TextEditor;
            let editor = cx.signal(editor_name, || TextEditor::new(""));
            let mirror = cx.signal(&format!("{editor_name}.text"), String::new);
            let find = cx.signal(&format!("{name}.find"), String::new);
            let needle = find.get(cx.runtime());
            let count = crate::richdoc::RichDoc::find(&mirror.get(cx.runtime()), &needle).len();

            let replace = cx.signal(&format!("{name}.replace"), String::new);
            let apply = {
                move |rt: &lumen_core::state::Runtime| {
                    let needle = find.get(rt);
                    let with = replace.get(rt);
                    if needle.is_empty() {
                        return;
                    }
                    editor.update(rt, |e| {
                        let (next, n) =
                            crate::richdoc::RichDoc::replace_all(e.text(), &needle, &with);
                        if n > 0 {
                            e.select_all();
                            e.insert(&next);
                        }
                    });
                    mirror.set(rt, editor.get(rt).text().to_string());
                }
            };

            let mut row = crate::widgets::row(vec![
                crate::widgets::text_field_basic(cx, &format!("{name}.find"), &needle)
                    .id(format!("{name}-find")),
                crate::widgets::text_field_basic(
                    cx,
                    &format!("{name}.replace"),
                    &replace.get(cx.runtime()),
                )
                .id(format!("{name}-replace")),
                crate::widgets::text(format!("{count} match(es)")).id(format!("{name}-count")),
                crate::widgets::button("Replace all", apply).id(format!("{name}-apply")),
            ]);
            row.style.column_gap = Dim::px(8.0);
            row
        };
        FindReplaceBar { el }
    }
}

impl_common!(FindReplaceBar);

/// M.4: a find/replace bar operating on a [`rich_text_editor`]'s source.
/// `{name}.find` / `{name}.replace` hold the inputs; the count label shows
/// live match counts and the button rewrites every occurrence (caret resets
/// to the end; the editor's undo history keeps the previous text).
/// *(Thin shim over [`FindReplaceBar`] — the typed form is preferred.)*
pub fn find_replace_bar(cx: &BuildCx, name: &str, editor_name: &str) -> Element {
    FindReplaceBar::new(cx, name, editor_name).into()
}
