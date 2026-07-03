//! datagrid — a virtualized spreadsheet whose rows and columns **grow as you
//! scroll**, like Excel: there is no fixed extent, only a `u32` index space
//! (~4 billion each way), and new cells are materialized as they scroll into
//! view. The top-left 100 × 100 block is seeded with numbers.
//!
//! Only the cells in the viewport are materialized (a few hundred `Element`
//! nodes), so the grid stays O(viewport) no matter how far you scroll:
//! * **Mouse wheel** scrolls (vertical, and horizontal via a trackpad / shift);
//!   scrolling toward an edge simply reveals the next rows/columns.
//! * **Ctrl/Cmd + wheel** zooms the grid in and out (scales cell sizes + font).
//! * **Draggable scrollbars** on the right and bottom edges.
//! * **Click a cell to edit it** — a text field opens in place; Enter or clicking
//!   another cell commits. Edits are stored sparsely and override the seeded
//!   value.
//! * **Drag a column or row header's trailing edge** to resize just that
//!   column/row.
//!
//! Geometry (overrides, positions, scroll offsets) is all kept in *content*
//! units; zoom multiplies to screen pixels at the boundary. `just run datagrid`.
use std::rc::Rc;

use lumen_core::events::Modifiers;
use lumen_core::state::{Runtime, Signal};
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use lumen_text::TextEditor;
use lumen_widgets::{widgets, App, BuildCx, Element, TextInput};

/// The top-left `DATA × DATA` cells hold numbers; the rest are empty. Indices
/// run over the whole `u32` space, so the grid has no fixed row/column count.
const DATA: u32 = 100;
const DW: f64 = 64.0; // default column width (content units)
const DH: f64 = 24.0; // default row height (content units)
const HDR_W: f64 = 48.0; // row-number header column width (screen px, unscaled)
const HDR_H: f64 = 26.0; // column-letter header row height (screen px, unscaled)
const TOOLBAR: f64 = 46.0;
const GRID_PX: f64 = 1.0; // gridline thickness (cells inset by this on r/b edges)
const SB: f64 = 12.0; // scrollbar thickness
const ZMIN: f64 = 0.6;
const ZMAX: f64 = 2.5;

/// Sparse edited-cell values, sorted by `(row, col)`. Kept a `Vec` (not a map)
/// so it stays a plain serializable `State` type.
type Edits = Vec<(u32, u32, String)>;

// Palette (built in `build`; `srgb8` isn't `const`).
struct Palette {
    grid: Color, // gridline colour == viewport background, shows through cell insets
    cell: Color,
    hdr: Color,
    corner: Color,
    ink: Color,
    muted: Color,
    track: Color,
    thumb: Color,
    edit: Color,
    edit_ink: Color,
}

fn palette() -> Palette {
    Palette {
        grid: Color::srgb8(0x2c, 0x34, 0x44, 0xff),
        cell: Color::srgb8(0x16, 0x19, 0x22, 0xff),
        hdr: Color::srgb8(0x23, 0x2c, 0x3e, 0xff),
        corner: Color::srgb8(0x2a, 0x33, 0x46, 0xff),
        ink: Color::srgb8(0xd6, 0xdc, 0xea, 0xff),
        muted: Color::srgb8(0x8b, 0x94, 0xa7, 0xff),
        track: Color::srgb8(0x1b, 0x20, 0x2b, 0xff),
        thumb: Color::srgb8(0x45, 0x50, 0x68, 0xff),
        edit: Color::srgb8(0xf6, 0xf8, 0xfd, 0xff),
        edit_ink: Color::srgb8(0x10, 0x14, 0x1c, 0xff),
    }
}

/// Build the app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

// --- resizable axis: a default size + sparse per-index overrides (content units)

fn size_of(over: &[(u32, f64)], def: f64, i: u32) -> f64 {
    over.binary_search_by_key(&i, |&(k, _)| k)
        .map(|j| over[j].1)
        .unwrap_or(def)
}

/// Start offset of index `i` = Σ size(k) for k < i.
fn pos_of(over: &[(u32, f64)], def: f64, i: u32) -> f64 {
    let corr: f64 = over
        .iter()
        .take_while(|(k, _)| *k < i)
        .map(|(_, w)| w - def)
        .sum();
    i as f64 * def + corr
}

/// Largest index whose start offset is `<= x` (binary search over `pos_of`).
/// `lo`/`hi` are `u64` so the midpoint can't overflow at the top of `u32`.
fn index_at(over: &[(u32, f64)], def: f64, x: f64) -> u32 {
    let (mut lo, mut hi) = (0u64, u32::MAX as u64);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if pos_of(over, def, mid as u32) <= x {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo as u32
}

fn set_override(over: &mut Vec<(u32, f64)>, i: u32, w: f64) {
    match over.binary_search_by_key(&i, |&(k, _)| k) {
        Ok(j) => over[j].1 = w,
        Err(j) => over.insert(j, (i, w)),
    }
}

// --- edits (sparse per-cell string values) -----------------------------------

fn edit_at(v: &[(u32, u32, String)], r: u32, c: u32) -> Option<&str> {
    v.binary_search_by(|e| (e.0, e.1).cmp(&(r, c)))
        .ok()
        .map(|i| v[i].2.as_str())
}

fn set_edit(v: &mut Edits, r: u32, c: u32, s: String) {
    match v.binary_search_by(|e| (e.0, e.1).cmp(&(r, c))) {
        Ok(i) if s.is_empty() => {
            v.remove(i);
        }
        Ok(i) => v[i].2 = s,
        Err(i) if !s.is_empty() => v.insert(i, (r, c, s)),
        Err(_) => {}
    }
}

/// The value shown in `(r, c)`: an edit if present, else the seeded number.
fn display_val(v: &[(u32, u32, String)], r: u32, c: u32) -> String {
    if let Some(s) = edit_at(v, r, c) {
        return s.to_string();
    }
    cell_val(r, c).map(|n| n.to_string()).unwrap_or_default()
}

/// The seeded value in cell `(r, c)` — only the top-left `DATA × DATA` block.
fn cell_val(r: u32, c: u32) -> Option<i64> {
    (r < DATA && c < DATA).then(|| {
        let (r, c) = (r as i64, c as i64);
        (r * 7 + c * 13 + (r * c) % 11) % 100
    })
}

/// Column letter(s): A..Z, AA...
fn col_name(mut c: u32) -> String {
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

/// Commit whatever is in the shared editor into the currently-editing cell.
fn commit_current(
    rt: &Runtime,
    editing: Signal<Option<(u32, u32)>>,
    edits: Signal<Edits>,
    editor: Signal<TextEditor>,
) {
    if let Some((r, c)) = editing.get(rt) {
        let t = editor.get(rt).text().to_string();
        edits.update(rt, move |v| set_edit(v, r, c, t));
    }
}

/// An absolutely-positioned box at `(x, y)` sized `(w, h)` (screen px).
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

/// A filled cell: a box inset by the gridline width on its right/bottom edges,
/// so the gridline-coloured viewport shows through as a crisp 1px grid.
fn filled(x: f64, y: f64, w: f64, h: f64, bg: Color) -> Element {
    let mut e = boxed(x, y, (w - GRID_PX).max(0.0), (h - GRID_PX).max(0.0));
    e.background = Some(bg);
    e
}

/// A bare text leaf, sized to its glyphs. It goes *inside* a sized cell box (as
/// a child) rather than on the box itself: a text-bearing element sizes its own
/// height to the glyphs and ignores an explicit `height`, so putting the label
/// on the box directly would collapse a resized row and leave the extra space
/// empty. Keeping text in a child lets the box own its height.
fn label_el(s: String, font: f32, color: Color, weight: f32) -> Element {
    let mut e = Element {
        label: s.clone(),
        content: lumen_widgets::NodeContent::Text(s, Default::default()),
        ..Element::default()
    };
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = font;
        ts.color = color;
        ts.weight = weight;
    }
    e
}

/// Put a left-aligned, vertically-centred label inside a (sized) cell box.
fn text_in(mut cell: Element, s: String, font: f32, color: Color, weight: f32) -> Element {
    cell.style.padding = Edges {
        left: Dim::px(6.0),
        right: Dim::px(4.0),
        top: Dim::px(2.0),
        bottom: Dim::px(2.0),
    };
    cell.style.align_items = Some(Align::Center); // vertical-centre the label
    cell.children.push(label_el(s, font, color, weight));
    cell
}

fn build(cx: &mut BuildCx) -> Element {
    let sx = cx.signal("sx", || 0.0f64); // scroll offset, content units
    let sy = cx.signal("sy", || 0.0f64);
    let cw = cx.signal("cw", Vec::<(u32, f64)>::new); // column-width overrides
    let rh = cx.signal("rh", Vec::<(u32, f64)>::new); // row-height overrides
    let zoom = cx.signal("zoom", || 1.0f64);
    let edits = cx.signal("edits", Edits::new);
    let editing = cx.signal("editing", || None::<(u32, u32)>);
    // The single shared editor state (also used by the in-cell `TextInput`).
    let editor = cx.signal("editor", || TextEditor::new(""));
    let emir = cx.signal("editor.text", String::new);

    let p = palette();
    let z = zoom.get(cx.runtime()).clamp(ZMIN, ZMAX);
    let (ox, oy) = (sx.get(cx.runtime()), sy.get(cx.runtime()));
    let cwv = cw.get(cx.runtime());
    let rhv = rh.get(cx.runtime());
    let editsv = edits.get(cx.runtime());
    let editing_now = editing.get(cx.runtime());

    // Viewport in screen px, then in content units (÷ zoom).
    let win = cx.size();
    let vw = (win.width - HDR_W).max(0.0);
    let vh = (win.height - TOOLBAR - HDR_H).max(0.0);
    let (vwc, vhc) = (vw / z, vh / z);

    // Visible index ranges (+1 row/col of overscan each side).
    let c0 = index_at(&cwv, DW, ox).saturating_sub(1);
    let c1 = index_at(&cwv, DW, ox + vwc).saturating_add(2);
    let r0 = index_at(&rhv, DH, oy).saturating_sub(1);
    let r1 = index_at(&rhv, DH, oy + vhc).saturating_add(2);

    // Content → screen (headers occupy the top-left frozen band; zoom scales).
    let cx_of = |c: u32| HDR_W + (pos_of(&cwv, DW, c) - ox) * z;
    let ry_of = |r: u32| HDR_H + (pos_of(&rhv, DH, r) - oy) * z;
    let eff_w = |c: u32| size_of(&cwv, DW, c) * z;
    let eff_h = |r: u32| size_of(&rhv, DH, r) * z;
    let cell_font = (12.0 * z) as f32;

    let mut layers: Vec<Element> = Vec::new();

    // Data cells (materialized only for the visible window). Each is focusable
    // and, when clicked, becomes the active editor; the click also focuses it
    // (per-cell id), so keystrokes route to the editor after the rebuild.
    for r in r0..r1 {
        let h = eff_h(r);
        let y = ry_of(r);
        for c in c0..c1 {
            let w = eff_w(c);
            let x = cx_of(c);
            if editing_now == Some((r, c)) {
                layers.push(editor_cell(
                    cx, r, c, x, y, w, h, cell_font, &p, editing, edits,
                ));
            } else {
                let mut cell = filled(x, y, w, h, p.cell);
                cell.id = Some(format!("cell-{r}-{c}").into());
                cell.focusable = true;
                cell.on_click = Some(start_edit(r, c, editing, edits, editor, emir));
                let v = display_val(&editsv, r, c);
                if !v.is_empty() {
                    cell = text_in(cell, v, cell_font, p.ink, 400.0);
                }
                layers.push(cell);
            }
        }
    }

    // Column headers (stable ids so their resized box is addressable).
    for c in c0..c1 {
        let mut hdr = text_in(
            filled(cx_of(c), 0.0, eff_w(c), HDR_H, p.hdr),
            col_name(c),
            11.0,
            p.muted,
            700.0,
        );
        hdr.id = Some(format!("ch-{c}").into());
        layers.push(hdr);
    }
    // Row headers.
    for r in r0..r1 {
        let mut hdr = text_in(
            filled(0.0, ry_of(r), HDR_W, eff_h(r), p.hdr),
            format!("{}", r + 1),
            11.0,
            p.muted,
            700.0,
        );
        hdr.id = Some(format!("rh-{r}").into());
        layers.push(hdr);
    }
    // Resize handles last, so they sit above the neighbouring header for hit-testing.
    for c in c0..c1 {
        layers.push(col_handle(
            c,
            pos_of(&cwv, DW, c),
            cx_of(c) + eff_w(c),
            sx,
            zoom,
            cw,
        ));
    }
    for r in r0..r1 {
        layers.push(row_handle(
            r,
            pos_of(&rhv, DH, r),
            ry_of(r) + eff_h(r),
            sy,
            zoom,
            rh,
        ));
    }
    // Frozen top-left corner.
    layers.push(filled(0.0, 0.0, HDR_W, HDR_H, p.corner));

    // Scrollbars (content extent = the seeded block, grown to include where we
    // are so the thumb keeps meaning past the filled area).
    let content_h = pos_of(&rhv, DH, DATA).max(oy + vhc);
    let content_w = pos_of(&cwv, DW, DATA).max(ox + vwc);
    layers.push(vscrollbar(win.width - SB, vh, content_h, oy, vhc, &p, sy));
    layers.push(hscrollbar(vh - SB + HDR_H, vw, content_w, ox, vwc, &p, sx));

    // The viewport: a clipped relative box whose gridline-coloured background
    // shows through the 1px cell insets. It owns the wheel handler.
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
    viewport.background = Some(p.grid);
    viewport.on_wheel = Some(Rc::new(move |rt, dx, dy, mods| {
        if mods.contains(Modifiers::CTRL) || mods.contains(Modifiers::META) {
            // Ctrl/Cmd + wheel → zoom (wheel-up zooms in).
            zoom.update(rt, |zz| *zz = (*zz * (1.0 - dy * 0.0016)).clamp(ZMIN, ZMAX));
        } else {
            let zz = zoom.get(rt).clamp(ZMIN, ZMAX);
            sy.update(rt, |o| *o = (*o + dy / zz).max(0.0));
            sx.update(rt, |o| *o = (*o + dx / zz).max(0.0));
        }
    }));

    // Toolbar: title + a live readout of zoom / position / edit state.
    let status = match editing_now {
        Some((r, c)) => format!("editing {}{}", col_name(c), r + 1),
        None => "click a cell to edit".to_string(),
    };
    let info = text_in(
        Element::default(),
        format!(
            "{}%  ·  rows {}–{}, cols {}–{}  ·  {}×{} seeded  ·  ctrl+wheel zoom · {}",
            (z * 100.0).round() as i64,
            r0 + 1,
            r1,
            col_name(c0),
            col_name(c1.saturating_sub(1)),
            DATA,
            DATA,
            status,
        ),
        12.0,
        p.muted,
        400.0,
    );
    let mut title = text_in(Element::default(), "Data grid".into(), 17.0, p.ink, 800.0);
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

/// The on-click handler that starts editing cell `(r, c)`: commit any current
/// edit, then seed the shared editor with this cell's shown value.
fn start_edit(
    r: u32,
    c: u32,
    editing: Signal<Option<(u32, u32)>>,
    edits: Signal<Edits>,
    editor: Signal<TextEditor>,
    emir: Signal<String>,
) -> Rc<dyn Fn(&Runtime)> {
    Rc::new(move |rt| {
        commit_current(rt, editing, edits, editor);
        let seed = display_val(&edits.get(rt), r, c);
        editor.set(rt, TextEditor::new(&seed));
        emir.set(rt, seed);
        editing.set(rt, Some((r, c)));
    })
}

/// The active cell rendered as an in-place `TextInput` (shares the "editor"
/// state). Same per-cell id as the static cell, so the click that started the
/// edit keeps focus here across the rebuild.
#[allow(clippy::too_many_arguments)]
fn editor_cell(
    cx: &BuildCx,
    r: u32,
    c: u32,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    font: f32,
    p: &Palette,
    editing: Signal<Option<(u32, u32)>>,
    edits: Signal<Edits>,
) -> Element {
    let mut ed: Element = TextInput::new(cx, "editor", "")
        .id(format!("cell-{r}-{c}"))
        .on_submit(move |rt, val| {
            let v = val.to_string();
            edits.update(rt, move |m| set_edit(m, r, c, v));
            editing.set(rt, None);
        })
        .into();
    ed.style.position = Position::Absolute;
    ed.style.inset = Edges {
        left: Dim::px(x as f32),
        top: Dim::px(y as f32),
        ..Edges::AUTO
    };
    ed.style.width = Dim::px((w - GRID_PX).max(0.0) as f32);
    ed.style.height = Dim::px((h - GRID_PX).max(0.0) as f32);
    ed.style.min_width = Dim::px(0.0);
    ed.style.padding = Edges {
        left: Dim::px(6.0),
        right: Dim::px(4.0),
        top: Dim::px(2.0),
        bottom: Dim::px(2.0),
    };
    ed.corner_radius = 0.0;
    ed.background = Some(p.edit);
    if let Some(ts) = ed.text_style_mut() {
        ts.font_size = font;
        ts.color = p.edit_ink;
    }
    ed
}

/// A draggable strip centred on a column's right border that resizes that
/// column. `left_content` is the column's start in content coords; the drag maps
/// the pointer to content x (÷ zoom) and sets the new width. Invisible (the
/// header edge is the affordance) but hit-testable via `on_drag`.
fn col_handle(
    c: u32,
    left_content: f64,
    border_x: f64,
    sx: Signal<f64>,
    zoom: Signal<f64>,
    cw: Signal<Vec<(u32, f64)>>,
) -> Element {
    let mut e = boxed(border_x - 3.5, 0.0, 7.0, HDR_H);
    e.id = Some(format!("cx-{c}").into());
    e.on_drag = Some(Rc::new(move |rt, _fx, _fy, pos| {
        let z = zoom.get(rt).clamp(ZMIN, ZMAX);
        let content_x = sx.get(rt) + (pos.x - HDR_W) / z;
        let neww = (content_x - left_content).clamp(24.0, 400.0);
        cw.update(rt, move |v| set_override(v, c, neww));
    }));
    e
}

/// A draggable strip centred on a row's bottom border that resizes that row.
fn row_handle(
    r: u32,
    top_content: f64,
    border_y: f64,
    sy: Signal<f64>,
    zoom: Signal<f64>,
    rh: Signal<Vec<(u32, f64)>>,
) -> Element {
    let mut e = boxed(0.0, border_y - 3.5, HDR_W, 7.0);
    e.id = Some(format!("ry-{r}").into());
    e.on_drag = Some(Rc::new(move |rt, _fx, _fy, pos| {
        let z = zoom.get(rt).clamp(ZMIN, ZMAX);
        let content_y = sy.get(rt) + (pos.y - TOOLBAR - HDR_H) / z;
        let newh = (content_y - top_content).clamp(16.0, 240.0);
        rh.update(rt, move |v| set_override(v, r, newh));
    }));
    e
}

/// Vertical scrollbar down the right edge: a track with a draggable thumb whose
/// length/position reflect the viewport window over the content height (both in
/// content units). `track_h`/`x` are screen px.
#[allow(clippy::too_many_arguments)]
fn vscrollbar(
    x: f64,
    track_h: f64,
    content_h: f64,
    oy: f64,
    vhc: f64,
    p: &Palette,
    sy: Signal<f64>,
) -> Element {
    let thumb_frac = (vhc / content_h).clamp(0.06, 1.0);
    let thumb_h = (thumb_frac * track_h).max(24.0);
    let span = content_h - vhc;
    let pos_frac = if span > 0.0 {
        (oy / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let rel_y = pos_frac * (track_h - thumb_h); // relative to the track's top

    let mut track = boxed(x, HDR_H, SB, track_h);
    track.background = Some(p.track);
    let mut thumb = boxed(2.0, rel_y, SB - 4.0, thumb_h);
    thumb.background = Some(p.thumb);
    thumb.corner_radius = (SB - 4.0) / 2.0;
    thumb.id = Some("vthumb".into());
    let travel = track_h - thumb_h;
    thumb.on_drag = Some(Rc::new(move |rt, _fx, _fy, pos| {
        let frac = if travel > 0.0 {
            ((pos.y - TOOLBAR - HDR_H) / travel).clamp(0.0, 1.0)
        } else {
            0.0
        };
        sy.set(rt, (frac * span).max(0.0));
    }));
    track.children.push(thumb);
    track
}

/// Horizontal scrollbar along the bottom edge.
#[allow(clippy::too_many_arguments)]
fn hscrollbar(
    y: f64,
    track_w: f64,
    content_w: f64,
    ox: f64,
    vwc: f64,
    p: &Palette,
    sx: Signal<f64>,
) -> Element {
    let thumb_frac = (vwc / content_w).clamp(0.06, 1.0);
    let thumb_w = (thumb_frac * track_w).max(24.0);
    let span = content_w - vwc;
    let pos_frac = if span > 0.0 {
        (ox / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let rel_x = pos_frac * (track_w - thumb_w); // relative to the track's left

    let mut track = boxed(HDR_W, y, track_w, SB);
    track.background = Some(p.track);
    let mut thumb = boxed(rel_x, 2.0, thumb_w, SB - 4.0);
    thumb.background = Some(p.thumb);
    thumb.corner_radius = (SB - 4.0) / 2.0;
    thumb.id = Some("hthumb".into());
    let travel = track_w - thumb_w;
    thumb.on_drag = Some(Rc::new(move |rt, _fx, _fy, pos| {
        let frac = if travel > 0.0 {
            ((pos.x - HDR_W) / travel).clamp(0.0, 1.0)
        } else {
            0.0
        };
        sx.set(rt, (frac * span).max(0.0));
    }));
    track.children.push(thumb);
    track
}
