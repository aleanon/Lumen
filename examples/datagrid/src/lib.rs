//! datagrid — a spreadsheet built on the reusable [`Grid`] widget.
//!
//! The [`Grid`] owns the mechanics — virtualization over a `u32 × u32` space,
//! 2D scroll + scrollbars, Ctrl/Cmd+wheel zoom, frozen headers, drag-to-resize,
//! gridlines. This example supplies only the *spreadsheet* part: what each cell
//! shows (a seeded number, or an edit), and in-place editing. The top-left
//! 100 × 100 block is seeded with numbers; every visible cell is click-to-edit.
//!
//! * **Click a cell** to edit it in place; Enter or clicking another cell commits
//!   into a sparse edits map that overrides the seeded value.
//! * **Wheel** scrolls (Shift ↔ horizontal); **Ctrl/Cmd + wheel** zooms; the
//!   right/bottom **scrollbars** and header-edge **resize** come from the widget.
//!
//! `just run datagrid`.
use std::rc::Rc;

use lumen_core::state::{Runtime, Signal};
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};
use lumen_text::TextEditor;
use lumen_widgets::{widgets, App, BuildCx, CellRef, Element, Grid, TextInput};

/// The top-left `DATA × DATA` cells are seeded with numbers; the rest are empty.
const DATA: u32 = 100;
const DW: f64 = 64.0; // default column width
const DH: f64 = 24.0; // default row height
const HDR_W: f64 = 48.0; // row-number header column width
const HDR_H: f64 = 26.0; // column-letter header row height
const TOOLBAR: f64 = 46.0;

// App content colours. The Grid's own palette (gridlines/cells/headers/thumbs)
// is `GridStyle::default()`, whose dark theme these are chosen to match.
#[derive(Clone, Copy)]
struct Colors {
    ink: Color,
    muted: Color,
    edit_bg: Color,
    edit_ink: Color,
}

fn colors() -> Colors {
    Colors {
        ink: Color::srgb8(0xd6, 0xdc, 0xea, 0xff),
        muted: Color::srgb8(0x8b, 0x94, 0xa7, 0xff),
        edit_bg: Color::srgb8(0xf6, 0xf8, 0xfd, 0xff),
        edit_ink: Color::srgb8(0x10, 0x14, 0x1c, 0xff),
    }
}

/// Sparse edited-cell values, sorted by `(row, col)`. A `Vec` (not a map) keeps
/// it a plain serializable `State` type.
type Edits = Vec<(u32, u32, String)>;

/// Build the app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
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

/// A bare text leaf, sized to its glyphs. It goes *inside* a sized cell box (as a
/// child) rather than on the box itself — a text-bearing element sizes its own
/// height to the glyphs and ignores an explicit height, so a label on the box
/// directly would collapse a resized row. Keeping text in a child lets the box
/// (which the Grid sizes) own its height.
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

/// Give a cell box a left-aligned, vertically-centred label.
fn text_in(mut cell: Element, s: String, font: f32, color: Color, weight: f32) -> Element {
    cell.style.padding = Edges {
        left: Dim::px(6.0),
        right: Dim::px(4.0),
        top: Dim::px(2.0),
        bottom: Dim::px(2.0),
    };
    cell.style.align_items = Some(Align::Center);
    cell.children.push(label_el(s, font, color, weight));
    cell
}

fn build(cx: &mut BuildCx) -> Element {
    // App state: sparse edits, which cell is being edited, and the shared editor.
    let edits = cx.signal("edits", Edits::new);
    let editing = cx.signal("editing", || None::<(u32, u32)>);
    let editor = cx.signal("editor", || TextEditor::new(""));
    let emir = cx.signal("editor.text", String::new);

    let win = cx.size();
    let col = colors();

    // The grid owns all the mechanics; we only say what each cell holds.
    let grid = Grid::new("sheet", u32::MAX, u32::MAX, DW, DH)
        .viewport(0.0, TOOLBAR, win.width, win.height - TOOLBAR)
        .col_header(HDR_H, move |c| {
            text_in(Element::default(), col_name(c), 11.0, col.muted, 700.0)
        })
        .row_header(HDR_W, move |r| {
            text_in(
                Element::default(),
                format!("{}", r + 1),
                11.0,
                col.muted,
                700.0,
            )
        })
        .resizable(true)
        .zoomable(true)
        .extent(DATA, DATA)
        .cell(move |cx, cell| Some(cell_view(cx, cell, col, editing, edits, editor, emir)))
        .build(&*cx);

    // Toolbar: title + a live readout of zoom / edit state.
    let z = Grid::zoom_of(&*cx, "sheet");
    let status = match editing.get(cx.runtime()) {
        Some((r, c)) => format!("editing {}{}", col_name(c), r + 1),
        None => "click a cell to edit".to_string(),
    };
    let info = text_in(
        Element::default(),
        format!(
            "{}%  ·  ctrl+wheel zoom · shift+wheel ↔ · {}",
            (z * 100.0).round() as i64,
            status
        ),
        12.0,
        col.muted,
        400.0,
    );
    let mut title = text_in(Element::default(), "Data grid".into(), 17.0, col.ink, 800.0);
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
        children: vec![bar, grid],
        ..Element::default()
    }
}

/// One cell's content: the active editor if this cell is being edited, otherwise
/// a focusable label that starts editing when clicked. The Grid sizes/positions
/// and ids it; the click focuses that id, so keystrokes route to the editor after
/// the rebuild.
#[allow(clippy::too_many_arguments)]
fn cell_view(
    cx: &BuildCx,
    cell: CellRef,
    col: Colors,
    editing: Signal<Option<(u32, u32)>>,
    edits: Signal<Edits>,
    editor: Signal<TextEditor>,
    emir: Signal<String>,
) -> Element {
    let (r, c) = (cell.row, cell.col);
    let font = (12.0 * cell.zoom) as f32;
    if editing.get(cx.runtime()) == Some((r, c)) {
        return editor_view(cx, r, c, font, col, editing, edits);
    }
    let mut el = Element {
        focusable: true,
        on_click: Some(start_edit(r, c, editing, edits, editor, emir)),
        ..Element::default()
    };
    let v = display_val(&edits.get(cx.runtime()), r, c);
    if !v.is_empty() {
        el = text_in(el, v, font, col.ink, 400.0);
    }
    el
}

/// The active cell as an in-place `TextInput` sharing the "editor" state. Enter
/// commits into the edits map and leaves edit mode.
fn editor_view(
    cx: &BuildCx,
    r: u32,
    c: u32,
    font: f32,
    col: Colors,
    editing: Signal<Option<(u32, u32)>>,
    edits: Signal<Edits>,
) -> Element {
    let mut ed: Element = TextInput::new(cx, "editor", "")
        .on_submit(move |rt, val| {
            let v = val.to_string();
            edits.update(rt, move |m| set_edit(m, r, c, v));
            editing.set(rt, None);
        })
        .into();
    ed.corner_radius = 0.0;
    ed.background = Some(col.edit_bg);
    ed.style.padding = Edges {
        left: Dim::px(6.0),
        right: Dim::px(4.0),
        top: Dim::px(2.0),
        bottom: Dim::px(2.0),
    };
    if let Some(ts) = ed.text_style_mut() {
        ts.font_size = font;
        ts.color = col.edit_ink;
    }
    ed
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
        // Commit whatever is in the editor into the previously-editing cell.
        if let Some((pr, pc)) = editing.get(rt) {
            let t = editor.get(rt).text().to_string();
            edits.update(rt, move |v| set_edit(v, pr, pc, t));
        }
        let seed = display_val(&edits.get(rt), r, c);
        editor.set(rt, TextEditor::new(&seed));
        emir.set(rt, seed);
        editing.set(rt, Some((r, c)));
    })
}
