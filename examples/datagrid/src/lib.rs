//! datagrid — a virtualized spreadsheet over a **65535 × 65535** grid (`u16` max
//! rows and columns), with the top-left 100 × 100 filled with numbers.
//!
//! Only the cells in the viewport are materialized (a few hundred `Element`
//! nodes), so navigating a grid of ~4 billion conceptual cells stays O(viewport):
//! * **Mouse wheel** scrolls (vertical, and horizontal via a trackpad / shift).
//! * **Drag a column or row header's edge** to resize just that column/row (the
//!   thin handle on the right/bottom of each header).
//!
//! Each visible cell is static text, so its glyph run is reused from the cache
//! (R5) as you scroll a row into view again. `just run datagrid`.
use std::rc::Rc;

use lumen_core::state::Signal;
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use lumen_widgets::{widgets, App, BuildCx, Element};

/// The grid is `u16::MAX` rows × `u16::MAX` columns.
const MAX: u16 = u16::MAX;
/// The top-left `DATA × DATA` cells hold numbers; the rest are empty.
const DATA: u16 = 100;
const DW: f64 = 64.0; // default column width
const DH: f64 = 24.0; // default row height
const HDR_W: f64 = 48.0; // row-number header column width
const HDR_H: f64 = 26.0; // column-letter header row height
const TOOLBAR: f64 = 46.0;
// The window the example opens at (build runs before layout, so the viewport is
// assumed from the run size; overscan hides minor mismatches).
const WIN_W: f64 = 1000.0;
const WIN_H: f64 = 700.0;

/// Build the app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

// --- resizable axis: a default size + sparse per-index overrides -------------
// Overrides live in a signal as a `Vec<(index, size)>` kept sorted by index.

fn size_of(over: &[(u16, f64)], def: f64, i: u16) -> f64 {
    over.binary_search_by_key(&i, |&(k, _)| k)
        .map(|j| over[j].1)
        .unwrap_or(def)
}

/// Start offset of index `i` = Σ size(k) for k < i.
fn pos_of(over: &[(u16, f64)], def: f64, i: u16) -> f64 {
    let corr: f64 = over
        .iter()
        .take_while(|(k, _)| *k < i)
        .map(|(_, w)| w - def)
        .sum();
    i as f64 * def + corr
}

/// Largest index whose start offset is `<= x` (binary search over `pos_of`).
fn index_at(over: &[(u16, f64)], def: f64, x: f64) -> u16 {
    let (mut lo, mut hi) = (0u32, MAX as u32);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if pos_of(over, def, mid as u16) <= x {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo as u16
}

fn set_override(over: &mut Vec<(u16, f64)>, i: u16, w: f64) {
    match over.binary_search_by_key(&i, |&(k, _)| k) {
        Ok(j) => over[j].1 = w,
        Err(j) => over.insert(j, (i, w)),
    }
}

/// The value in cell `(r, c)` — only the top-left `DATA × DATA` block is filled.
fn cell_val(r: u16, c: u16) -> Option<i64> {
    (r < DATA && c < DATA).then(|| {
        let (r, c) = (r as i64, c as i64);
        (r * 7 + c * 13 + (r * c) % 11) % 100
    })
}

/// Column letter(s): A..Z, AA...
fn col_name(mut c: u16) -> String {
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

/// An absolutely-positioned box at `(x, y)` sized `(w, h)`.
fn boxed(x: f64, y: f64, w: f64, h: f64) -> Element {
    let mut e = Element::default();
    e.style.position = Position::Absolute;
    e.style.inset = Edges {
        left: Dim::px(x as f32),
        top: Dim::px(y as f32),
        ..Edges::AUTO
    };
    e.style.width = Dim::px(w as f32);
    e.style.height = Dim::px(h as f32);
    e
}

fn text_in(mut e: Element, s: String, font: f32, color: Color, weight: f32) -> Element {
    e.label = s.clone();
    e.content = lumen_widgets::NodeContent::Text(s, Default::default());
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = font;
        ts.color = color;
        ts.weight = weight;
    }
    e.style.padding = Edges {
        left: Dim::px(6.0),
        right: Dim::px(4.0),
        top: Dim::px(3.0),
        bottom: Dim::px(2.0),
    };
    e.style.align_items = Some(Align::Center);
    e
}

fn build(cx: &mut BuildCx) -> Element {
    let sx = cx.signal("sx", || 0.0f64);
    let sy = cx.signal("sy", || 0.0f64);
    let cw = cx.signal("cw", Vec::<(u16, f64)>::new); // column-width overrides
    let rh = cx.signal("rh", Vec::<(u16, f64)>::new); // row-height overrides

    let (ox, oy) = (sx.get(cx.runtime()), sy.get(cx.runtime()));
    let cwv = cw.get(cx.runtime());
    let rhv = rh.get(cx.runtime());

    // Content viewport (grid area minus the frozen headers).
    let vw = WIN_W - HDR_W;
    let vh = WIN_H - TOOLBAR - HDR_H;

    // Visible index ranges (+1 row/col of overscan each side).
    let c0 = index_at(&cwv, DW, ox).saturating_sub(1);
    let c1 = index_at(&cwv, DW, ox + vw).saturating_add(2);
    let r0 = index_at(&rhv, DH, oy).saturating_sub(1);
    let r1 = index_at(&rhv, DH, oy + vh).saturating_add(2);

    let ink = Color::srgb8(0xd6, 0xdc, 0xea, 0xff);
    let muted = Color::srgb8(0x8b, 0x94, 0xa7, 0xff);

    // Screen x/y of a content position (headers occupy the top-left frozen band).
    let cx_of = |c: u16| HDR_W + pos_of(&cwv, DW, c) - ox;
    let ry_of = |r: u16| HDR_H + pos_of(&rhv, DH, r) - oy;

    let mut layers: Vec<Element> = Vec::new();

    // Data cells (materialized only for the visible window).
    for r in r0..r1 {
        let h = size_of(&rhv, DH, r);
        let y = ry_of(r);
        for c in c0..c1 {
            let w = size_of(&cwv, DW, c);
            let x = cx_of(c);
            let mut cell = boxed(x, y, w, h);
            cell.background = Some(Color::srgb8(0x16, 0x19, 0x22, 0xff));
            if let Some(v) = cell_val(r, c) {
                cell = text_in(cell, format!("{v}"), 12.0, ink, 400.0);
            }
            layers.push(cell);
        }
    }

    // Column headers.
    for c in c0..c1 {
        let w = size_of(&cwv, DW, c);
        let x = cx_of(c);
        let mut hdr = boxed(x, 0.0, w, HDR_H);
        hdr.background = Some(Color::srgb8(0x23, 0x2c, 0x3e, 0xff));
        layers.push(text_in(hdr, col_name(c), 11.0, muted, 700.0));
    }
    // Row headers.
    for r in r0..r1 {
        let h = size_of(&rhv, DH, r);
        let y = ry_of(r);
        let mut hdr = boxed(0.0, y, HDR_W, h);
        hdr.background = Some(Color::srgb8(0x23, 0x2c, 0x3e, 0xff));
        layers.push(text_in(
            hdr,
            format!("{}", r as u32 + 1),
            11.0,
            muted,
            700.0,
        ));
    }
    // Resize handles last, so they sit above the neighbouring header for hit-testing
    // (each handle straddles the border it owns and the next header's leading edge).
    for c in c0..c1 {
        let w = size_of(&cwv, DW, c);
        layers.push(col_handle(c, pos_of(&cwv, DW, c), cx_of(c) + w, sx, cw));
    }
    for r in r0..r1 {
        let h = size_of(&rhv, DH, r);
        layers.push(row_handle(r, pos_of(&rhv, DH, r), ry_of(r) + h, sy, rh));
    }
    // Frozen top-left corner.
    let mut corner = boxed(0.0, 0.0, HDR_W, HDR_H);
    corner.background = Some(Color::srgb8(0x2a, 0x33, 0x46, 0xff));
    layers.push(corner);

    // The viewport: a clipped relative box that owns the wheel handler.
    let mut viewport = Element {
        role: lumen_core::semantics::Role::Group,
        style: LayoutStyle {
            position: Position::Relative,
            width: Dim::pct(1.0),
            flex_grow: 1.0,
            min_height: Dim::px(0.0),
            ..LayoutStyle::default()
        },
        clip: true,
        children: layers,
        ..Element::default()
    };
    viewport.background = Some(Color::srgb8(0x0e, 0x11, 0x18, 0xff));
    viewport.on_wheel = Some(Rc::new(move |rt, dx, dy| {
        sy.update(rt, |o| *o = (*o + dy).max(0.0));
        sx.update(rt, |o| *o = (*o + dx).max(0.0));
    }));

    // Toolbar: a title + a live readout of what's on screen.
    let info = text_in(
        Element::default(),
        format!(
            "{MAX} × {MAX} grid · {}×{} filled · showing R{}–{} C{}–{} · {} cells live · wheel to scroll, drag a header edge to resize",
            DATA, DATA, r0 + 1, r1, col_name(c0), col_name(c1.saturating_sub(1)), (r1 - r0) as u32 * (c1 - c0) as u32
        ),
        12.0,
        muted,
        400.0,
    );
    let mut title = text_in(Element::default(), "Data grid".into(), 17.0, ink, 800.0);
    title.style.width = Dim::px(120.0);
    let mut spacer = Element::default();
    spacer.style.flex_grow = 1.0;
    let mut bar = widgets::row(vec![title, spacer, info]);
    bar.style.align_items = Some(Align::Center);
    bar.style.height = Dim::px(TOOLBAR as f32);
    bar.style.padding = Edges::all(Dim::px(12.0));
    bar.background = Some(Color::srgb8(0x16, 0x1c, 0x28, 0xff));

    Element {
        role: lumen_core::semantics::Role::Group,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Dim::pct(1.0),
            height: Dim::pct(1.0),
            ..LayoutStyle::default()
        },
        children: vec![bar, viewport],
        ..Element::default()
    }
}

/// A draggable strip on a column's right border (`border_x`, viewport-local) that
/// resizes that column. `left_content` is the column's start in content coords;
/// the drag sets `width = content_pointer_x − left_content`. The stable id lets
/// the drag survive the resize rebuilds (nodes get renumbered).
fn col_handle(
    c: u16,
    left_content: f64,
    border_x: f64,
    sx: Signal<f64>,
    cw: Signal<Vec<(u16, f64)>>,
) -> Element {
    let mut e = boxed(border_x - 3.5, 0.0, 7.0, HDR_H + 4.0);
    e.id = Some(format!("cx-{c}").into());
    e.background = Some(Color::srgb8(0x4d, 0x7c, 0xfe, 0x50));
    e.on_drag = Some(Rc::new(move |rt, _fx, _fy, pos| {
        // Viewport is at window x = 0; content x = pointer − row-header − scroll.
        let content_x = pos.x - HDR_W + sx.get(rt);
        let neww = (content_x - left_content).clamp(24.0, 400.0);
        cw.update(rt, move |v| set_override(v, c, neww));
    }));
    e
}

/// A draggable strip on a row's bottom border that resizes that row.
fn row_handle(
    r: u16,
    top_content: f64,
    border_y: f64,
    sy: Signal<f64>,
    rh: Signal<Vec<(u16, f64)>>,
) -> Element {
    let mut e = boxed(0.0, border_y - 3.5, HDR_W + 4.0, 7.0);
    e.id = Some(format!("ry-{r}").into());
    e.background = Some(Color::srgb8(0x4d, 0x7c, 0xfe, 0x50));
    e.on_drag = Some(Rc::new(move |rt, _fx, _fy, pos| {
        // Viewport starts below the toolbar; content y = pointer − toolbar −
        // col-header − scroll.
        let content_y = pos.y - TOOLBAR - HDR_H + sy.get(rt);
        let newh = (content_y - top_content).clamp(16.0, 240.0);
        rh.update(rt, move |v| set_override(v, r, newh));
    }));
    e
}
