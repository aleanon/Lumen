//! datagrid — a large spreadsheet-style grid (thousands of `Element` nodes) that
//! stays smooth thanks to the fine-grained runtime.
//!
//! * Each **row** is a `cx.scope`, so clicking a cell re-runs only that one row —
//!   the other rows reuse their cached subtrees (F1 memoization). Removed rows
//!   are swept (F5 GC), so changing the grid size doesn't leak state.
//! * Every cell is static text most of the time, so its glyph run is reused from
//!   the cache instead of rebuilt (R5) — a full-grid frame emits O(changed) draw
//!   commands, not O(cells).
//! * **Zoom** (`−`/`+`) rescales the grid: zooming *out* packs in more, smaller
//!   cells — from ~100 nodes up to ~8000 — to show the framework at scale.
//!
//! `just run datagrid` (window) / `just render datagrid` (headless PNG).
use std::rc::Rc;

use lumen_core::state::{Runtime, Signal};
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};
use lumen_widgets::{widgets, App, BuildCx, Element};

/// Build the data-grid app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

/// `(rows, cols, font_size, cell_w, cell_h)` for a zoom level. Higher level =
/// zoomed out = more, smaller cells.
fn dims(zoom: i64) -> (usize, usize, f32, f32, f32) {
    match zoom.clamp(0, 5) {
        0 => (12, 8, 15.0, 70.0, 30.0),
        1 => (20, 12, 13.0, 58.0, 26.0),
        2 => (32, 16, 11.0, 48.0, 22.0),
        3 => (48, 22, 9.0, 38.0, 18.0),
        4 => (72, 30, 7.5, 30.0, 15.0),
        _ => (100, 40, 6.5, 24.0, 13.0),
    }
}

/// A cell's base "data" value — deterministic, so most cells are static text
/// (maximising glyph-run reuse). Clicks add to it.
fn base(r: usize, c: usize) -> i64 {
    ((r * 7 + c * 13 + (r * c) % 11) % 100) as i64
}

/// A single grid cell: fixed-size text, a background (highlighted once clicked),
/// and a click handler that bumps its value.
fn cell(
    text: String,
    hot: bool,
    font: f32,
    w: f32,
    h: f32,
    on: impl Fn(&Runtime) + 'static,
) -> Element {
    let mut e = widgets::text(text);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = font;
        ts.color = Color::srgb8(0xd0, 0xd6, 0xe6, 0xff);
    }
    e.style.width = Dim::px(w);
    e.style.height = Dim::px(h);
    e.style.padding = Edges {
        left: Dim::px(5.0),
        right: Dim::px(5.0),
        top: Dim::px(2.0),
        bottom: Dim::px(2.0),
    };
    e.style.align_items = Some(Align::Center);
    e.background = Some(if hot {
        Color::srgb8(0x2b, 0x4c, 0x8f, 0xff)
    } else {
        Color::srgb8(0x1c, 0x1f, 0x2b, 0xff)
    });
    e.on_click = Some(Rc::new(on));
    e
}

/// A fixed-size header/label cell (no click).
fn label(text: impl Into<String>, font: f32, w: f32, h: f32, class: &str) -> Element {
    let mut e = widgets::text(text).class(class);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = font;
        ts.weight = 700.0;
    }
    e.style.width = Dim::px(w);
    e.style.height = Dim::px(h);
    e.style.padding = Edges {
        left: Dim::px(5.0),
        right: Dim::px(5.0),
        top: Dim::px(2.0),
        bottom: Dim::px(2.0),
    };
    e.style.align_items = Some(Align::Center);
    e
}

/// A horizontal band of cells with a 1px gap (the grid's row shape).
fn band(cells: Vec<Element>) -> Element {
    let mut r = widgets::row(cells);
    r.style.column_gap = Dim::px(1.0);
    r
}

/// The column letter for `c` (A..Z, then AA..).
fn col_name(c: usize) -> String {
    let mut c = c;
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (c % 26) as u8) as char);
        if c < 26 {
            break;
        }
        c = c / 26 - 1;
    }
    s
}

fn build(cx: &mut BuildCx) -> Element {
    let zoom = cx.signal("zoom", || 2i64);
    let z = zoom.get(cx.runtime());
    let (rows, cols, font, cw, ch) = dims(z);
    let row_hdr_w = cw * 0.7;
    let cells_total = rows * (cols + 1);

    // Header row: a corner + column letters.
    let mut header = vec![label("#", font, row_hdr_w, ch, "corner")];
    for c in 0..cols {
        header.push(label(col_name(c), font, cw, ch, "colhdr"));
    }
    let header = band(header);

    // Data rows — each is a scope, so a click re-runs only its row (F1); vanished
    // rows on zoom-in are swept (F5 GC).
    let data_rows = widgets::keyed(
        cx,
        0..rows,
        |r| format!("row-{r}"),
        move |cx, &r| {
            // Read `zoom` INSIDE the scope so it's a real dependency: a zoom change
            // (which changes the column count/size) must re-run every row — F1
            // tracks signal reads, not the captured `cols`/`font` values.
            let z = zoom.get(cx.runtime());
            let (_, cols, font, cw, ch) = dims(z);
            let row_hdr_w = cw * 0.7;
            // Per-row click counters (scope-local; padded to the current col count
            // so a zoom change that widens the grid is safe).
            let clicks: Signal<Vec<i64>> = cx.signal("clicks", move || vec![0i64; cols]);
            let cs = clicks.get(cx.runtime());
            let mut cells = vec![label(format!("{}", r + 1), font, row_hdr_w, ch, "rowhdr")];
            for c in 0..cols {
                let clk = cs.get(c).copied().unwrap_or(0);
                let val = base(r, c) + clk;
                cells.push(cell(format!("{val}"), clk > 0, font, cw, ch, move |rt| {
                    clicks.update(rt, move |v| {
                        if c >= v.len() {
                            v.resize(c + 1, 0);
                        }
                        v[c] += 1;
                    })
                }));
            }
            band(cells)
        },
    );

    let mut grid = widgets::column(data_rows);
    grid.style.row_gap = Dim::px(1.0);

    // The grid fills the remaining height and clips overflow — zooming changes how
    // many (smaller) cells are visible. Rows past the fold are still built + laid
    // out; the point is node count.
    let mut body = widgets::column(vec![header, grid]);
    body.style.flex_grow = 1.0;
    body.style.min_height = Dim::px(0.0);
    body.style.row_gap = Dim::px(1.0);
    body.clip = true;

    // Controls: zoom −/+, the level, and a live node count.
    let zoom_out = widgets::button("−", move |rt| zoom.update(rt, |z| *z = (*z + 1).min(5)))
        .class("zbtn")
        .id("zoom-out");
    let zoom_in = widgets::button("+", move |rt| zoom.update(rt, |z| *z = (*z - 1).max(0)))
        .class("zbtn")
        .id("zoom-in");
    let mut info = widgets::text(format!(
        "{rows} × {cols} = {cells_total} cells  ·  zoom {}/5  ·  click a cell to edit its row",
        z
    ))
    .class("info");
    if let Some(ts) = info.text_style_mut() {
        ts.font_size = 13.0;
    }
    let mut spacer = Element::default();
    spacer.style.flex_grow = 1.0;
    let mut bar = widgets::row(vec![title(), spacer, info, zoom_out, gap(8.0), zoom_in]);
    bar.style.align_items = Some(Align::Center);
    bar.style.column_gap = Dim::px(10.0);
    bar.style.padding = Edges::all(Dim::px(12.0));
    bar = bar.class("bar");

    Element {
        role: lumen_core::semantics::Role::Group,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Dim::pct(1.0),
            height: Dim::pct(1.0),
            ..LayoutStyle::default()
        },
        children: vec![bar, body],
        ..Element::default()
    }
    .class("app")
}

fn title() -> Element {
    let mut t = widgets::text("Data grid").class("title");
    if let Some(ts) = t.text_style_mut() {
        ts.font_size = 18.0;
        ts.weight = 800.0;
    }
    t
}

fn gap(w: f32) -> Element {
    let mut e = Element::default();
    e.style.width = Dim::px(w);
    e
}
