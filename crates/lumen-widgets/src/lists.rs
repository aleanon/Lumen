//! Row-oriented widgets: virtualized lists, data grids, trees and pagination.
//! All of them render only the rows on screen, which is why they share a file.
//!
//! (SD2: regrouped out of the milestone-named `widgets_m*`/`misc_w2` modules,
//! which recorded WHEN a widget was written rather than what it is.)

use crate::widget::impl_common;
use crate::{widgets, BuildCx, Element};
use lumen_core::semantics::{Action, Role, ScrollInfo, State as SemState};
use lumen_core::Color;
use lumen_layout::{Align as LAlign, Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use lumen_text::TextStyle;
use std::collections::HashSet;
use std::rc::Rc;

/// Page navigation: `‹ 1 2 … n ›`, current page in a signal.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Pagination, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Pagination::new(cx, "page", 5).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 280.0, 60.0, "pagination");
/// ```
///
/// Renders:
///
/// ![Pagination example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/pagination.png)
///
/// The picture above is `src/doc_shots/pagination.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Pagination {
    el: Element,
}

impl Pagination {
    /// A pager over `pages` pages (1-based); the current page lives in the
    /// `{name}.page` signal (`i64`, clamped).
    pub fn new(cx: &BuildCx, name: &str, pages: i64) -> Pagination {
        let pages = pages.max(1);
        let page = cx.signal(format!("{name}.page"), || 1i64);
        let cur = page.get(cx.runtime()).clamp(1, pages);

        let btn = |label: String, target: i64, active: bool, enabled: bool| {
            let mut b = widgets::text(label);
            if let Some(ts) = b.text_style_mut() {
                ts.font_size = 13.0;
                ts.color = if active {
                    Color::srgb8(0xff, 0xff, 0xff, 0xff)
                } else if enabled {
                    Color::srgb8(0x1c, 0x22, 0x30, 0xff)
                } else {
                    Color::srgb8(0xb3, 0xb9, 0xc2, 0xff)
                };
            }
            b.role = Role::Button;
            b.focusable = enabled;
            b.background = Some(if active {
                crate::theme::accent()
            } else {
                Color::srgb8(0xf3, 0xf5, 0xf8, 0xff)
            });
            b.corner_radius = 6.0;
            b.style.padding = Edges {
                left: Dim::px(9.0),
                right: Dim::px(9.0),
                top: Dim::px(4.0),
                bottom: Dim::px(4.0),
            };
            if enabled {
                b.on_click = Some(Rc::new(move |rt| {
                    page.set(rt, target.clamp(1, pages));
                }));
            }
            b
        };

        let mut children =
            vec![btn("‹".into(), cur - 1, false, cur > 1).id(format!("{name}-prev"))];
        for p in 1..=pages {
            children.push(btn(p.to_string(), p, p == cur, true).id(format!("{name}-p{p}")));
        }
        children.push(btn("›".into(), cur + 1, false, cur < pages).id(format!("{name}-next")));

        let mut row = widgets::row(children).class("pagination");
        row.role = Role::Group;
        row.style.column_gap = Dim::px(6.0);
        row.style.align_items = Some(LAlign::Center);
        Pagination { el: row }
    }
}

impl_common!(Pagination);

/// [`VirtualList`] — a windowing list materializing only visible items
/// plus overscan; scroll offset under `name` (typed form of
/// [`virtual_list`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, widgets, VirtualList, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let list =
///         VirtualList::new(cx, "vl", 1000, 24.0, 96.0, |i| widgets::text(format!("Row {i}")));
///     centered(cx, list.into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 200.0, 120.0, "virtual_list");
/// ```
///
/// Renders:
///
/// ![Virtual List example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/virtual_list.png)
///
/// The picture above is `src/doc_shots/virtual_list.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct VirtualList {
    el: Element,
}

/// The scroll window a `VirtualList` materializes: which items, and where the
/// first one sits. Shared by the plain and memoized constructors so the two
/// cannot drift — they differ only in how each row is produced.
struct Window {
    offset: lumen_core::state::Signal<f64>,
    y: f64,
    max_y: f64,
    first: usize,
    last: usize,
}

fn window(
    cx: &BuildCx,
    name: &str,
    item_count: usize,
    item_height: f64,
    viewport_h: f64,
) -> Window {
    const OVERSCAN: usize = 2;
    let offset = cx.signal(name, || 0.0f64);
    let max_y = (item_count as f64 * item_height - viewport_h).max(0.0);
    // Clamp on READ, like `Scrollable` does. Without it a stale offset
    // survives when the list shrinks, and — since the wheel router now
    // consults `ScrollInfo` to decide whether to chain — a `y > max_y`
    // would make this list look scrollable when it is stuck.
    let y = offset.get(cx.runtime()).clamp(0.0, max_y);
    let first = ((y / item_height).floor() as usize).saturating_sub(OVERSCAN);
    let per_view = (viewport_h / item_height).ceil() as usize;
    let last = (first + per_view + OVERSCAN * 2).min(item_count);
    Window {
        offset,
        y,
        max_y,
        first,
        last,
    }
}

/// Position one row in the window. Every row goes through this, memoized or not.
fn place(mut el: Element, i: usize, y: f64, item_height: f64) -> Element {
    let top = (i as f64 * item_height) - y;
    el.style.position = Position::Absolute;
    // `left` AND `right` pinned with `width: auto` stretches the item to the
    // viewport, which is what an author expects of a row. Setting `width: 100%`
    // unconditionally (the workaround consumers were writing) would clobber an
    // item that sets its own width; pinning both insets composes with it.
    el.style.inset = Edges {
        left: Dim::px(0.0),
        right: Dim::px(0.0),
        top: Dim::px(top as f32),
        ..Edges::AUTO
    };
    // …except the insets do NOT stretch a text-bearing row: a text element's
    // measure fixes its own width, so `|i| widgets::text(…)` — the most obvious
    // way to write a list — shrink-wrapped to its glyphs (42 px in a 300 px
    // list), and everything right of the label missed the row on a tap. A
    // *definite* width is resolved before measure runs, so it lands where the
    // insets cannot. Applied only when the caller left the width `Auto`, which
    // keeps the composition the insets were chosen for.
    if el.style.width == Dim::Auto {
        el.style.width = Dim::pct(1.0);
    }
    el.style.height = Dim::px(item_height as f32);
    el
}

/// Assemble the viewport around already-placed rows.
fn viewport(name: &str, w: &Window, viewport_h: f64, children: Vec<Element>) -> Element {
    let (offset, y, max_y) = (w.offset, w.y, w.max_y);
    Element {
        role: Role::List,
        // Overscan rows (up to two above and below) sit outside the viewport
        // box. Without clipping they PAINT and stay HIT-TESTABLE there — the
        // same defect `scroll_hit_clip.rs` exists for on `Scrollable`, which
        // has always set this.
        clip: true,
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
        on_wheel: Some(Rc::new(move |rt, _dx, dy, _mods| {
            offset.update(rt, |o| *o = (*o + dy).clamp(0.0, max_y))
        })),
        children: match crate::scrollable::overlay_scrollbar(name, viewport_h, y, max_y, offset) {
            Some(bar) => vec![crate::scrollable::gutter_plane(children), bar],
            None => children,
        },
        ..Element::default()
    }
}

impl VirtualList {
    /// A windowing list (02 §10): materializes only the visible items plus overscan,
    /// regardless of `item_count`. State (`name`) is the scroll offset.
    ///
    /// Rows are rebuilt every frame — and **that is usually the right default**.
    /// A virtual list only builds the ~44 rows in its window, so rebuilding them
    /// is a small part of a frame that is dominated by *rasterizing* them.
    /// [`memoized`](Self::memoized) skips unchanged row builders; measured, it
    /// buys **1.01× for a one-element row** and ~1.1× for a 16-element one.
    pub fn new(
        cx: &BuildCx,
        name: &str,
        item_count: usize,
        item_height: f64,
        viewport_h: f64,
        render: impl Fn(usize) -> Element,
    ) -> VirtualList {
        let w = window(cx, name, item_count, item_height, viewport_h);
        let children: Vec<Element> = (w.first..w.last)
            .map(|i| place(render(i), i, w.y, item_height))
            .collect();
        VirtualList {
            el: viewport(name, &w, viewport_h, children),
        }
    }

    /// [`new`](Self::new), but each row is memoized on `dep(i)` — an unchanged
    /// row is not rebuilt.
    ///
    /// **Measure before reaching for this.** Virtualization already bounds the
    /// work memoization would save: only the ~44 rows in the window are built at
    /// all, and the frame is dominated by rasterizing them. Measured on a
    /// 100 000-item list with one row changing per frame:
    ///
    /// | elements per row | speedup |
    /// |---:|---:|
    /// | 1 | **1.01×** |
    /// | 4 | 1.31× |
    /// | 16 | 1.10× |
    /// | 64 | 1.10× |
    ///
    /// So it is worth it when a row is *expensive to build* — parsing,
    /// formatting, many child elements — and worth nothing when it is a label.
    /// It also costs ~1 KB of retained `Element` per materialized row (~121 KiB
    /// for a full window, whatever the item count).
    ///
    /// **Why `dep` and not just the index.** A memoized row invalidates on the
    /// signals its closure *reads*, and the usual shape reads none: the caller
    /// pulls its data out of a signal, then captures it.
    ///
    /// ```ignore
    /// let items = items.get(cx.runtime());       // read HERE, in the parent
    /// VirtualList::memoized(cx, "l", items.len(), 24.0, 400.0,
    ///     |i| items[i].version,                  // …so state the dependency
    ///     |i| row_view(&items[i]))
    /// ```
    ///
    /// Without `dep` the row's read set is empty, which is always "current", and
    /// the row would freeze forever. `dep` is the same idea as `cx.task`'s deps,
    /// and `crates/lumen-widgets/tests/scope_deps.rs` asserts the hazard is real.
    ///
    /// A dep that is cheap to hash and changes exactly when the row's appearance
    /// changes is the right one — an item's version, its whole value if it is
    /// small and `Hash`, or `()` for a row that genuinely never changes.
    ///
    /// Row identity is the index, so scope-local state follows the *position*,
    /// not the item. For a reorderable list, prefer [`widgets::keyed`] over a
    /// virtual list, or make `dep` carry the item id.
    ///
    /// [`widgets::keyed`]: crate::widgets::keyed
    pub fn memoized<D: std::hash::Hash>(
        cx: &mut BuildCx,
        name: &str,
        item_count: usize,
        item_height: f64,
        viewport_h: f64,
        dep: impl Fn(usize) -> D,
        render: impl Fn(usize) -> Element,
    ) -> VirtualList {
        let w = window(cx, name, item_count, item_height, viewport_h);
        let children: Vec<Element> = (w.first..w.last)
            .map(|i| {
                let el = cx.scope_with_deps(("vlist-row", i), dep(i), |_cx| render(i));
                place(el, i, w.y, item_height)
            })
            .collect();
        VirtualList {
            el: viewport(name, &w, viewport_h, children),
        }
    }
}

impl_common!(VirtualList);

/// A windowing list (02 §10): materializes only the visible items plus overscan,
/// regardless of `item_count`. State (`name`) is the scroll offset.
/// *(Thin shim over [`VirtualList`] — the typed form is preferred.)*
pub fn virtual_list(
    cx: &BuildCx,
    name: &str,
    item_count: usize,
    item_height: f64,
    viewport_h: f64,
    render: impl Fn(usize) -> Element,
) -> Element {
    VirtualList::new(cx, name, item_count, item_height, viewport_h, render).into()
}

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
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, DataGrid, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let grid =
///         DataGrid::new(cx, "grid", &["Name", "Age"], 100, 24.0, 96.0, |r, c| format!("r{r}c{c}"));
///     centered(cx, grid.into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 220.0, 140.0, "data_grid");
/// ```
///
/// Renders:
///
/// ![Data Grid example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/data_grid.png)
///
/// The picture above is `src/doc_shots/data_grid.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
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
            // Clamped on read, like `Scrollable` — see the note in `VirtualList`.
            let max_y = (row_count as f64 * row_height - viewport_h).max(0.0);
            let y = offset.get(cx.runtime()).clamp(0.0, max_y);
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

            let body = Element {
                role: Role::Group,
                // Same as `VirtualList`: overscan rows outside the viewport must
                // not paint or take hits.
                clip: true,
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
                children: match crate::scrollable::overlay_scrollbar(
                    name, viewport_h, y, max_y, offset,
                ) {
                    Some(bar) => vec![crate::scrollable::gutter_plane(rows), bar],
                    None => rows,
                },
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
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::widgets::TreeRow;
/// use lumen_widgets::{centered, Tree, BuildCx, Element};
/// use std::collections::HashSet;
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let rows = [
///         TreeRow { id: "animals", label: "Animals", depth: 0, has_children: true },
///         TreeRow { id: "cat", label: "Cat", depth: 1, has_children: false },
///     ];
///     // Seed the expanded-id set so "animals" starts open and "Cat" shows.
///     cx.signal("tree", || HashSet::from(["animals".to_string()]));
///     centered(cx, Tree::new(cx, "tree", &rows).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 200.0, 110.0, "tree");
/// ```
///
/// Renders:
///
/// ![Tree example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/tree.png)
///
/// The picture above is `src/doc_shots/tree.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
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
                    // Only a parent row does anything on click. A leaf used to
                    // advertise `Click` regardless, which promised the agent and
                    // assistive tech an activation that would silently do
                    // nothing (W0106). Both kinds stay focusable and reachable.
                    actions: if toggleable {
                        vec![Action::Click, Action::Focus]
                    } else {
                        vec![Action::Focus]
                    },
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
