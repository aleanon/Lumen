//! multi_window — M.6/P.3d: two OS windows over one reactive store. The main
//! window edits a note; the "Preview" window renders it live — a write in
//! either window re-renders both (shared signals ARE the sync).
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use lumen_widgets::system::WindowDesc;
use lumen_widgets::{widgets, App, BuildCx, Element};

/// Build the multi-window app (main editor + declared preview window).
pub fn main_app() -> App {
    App::new(build).window(
        WindowDesc {
            id: "preview".into(),
            title: "Preview".into(),
            width: 320.0,
            height: 200.0,
        },
        preview,
    )
}

fn page(children: Vec<Element>) -> Element {
    let mut col = widgets::column(children).id("page");
    col.style = LayoutStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        width: Dim::pct(1.0),
        height: Dim::pct(1.0),
        align_items: Some(Align::Center),
        justify_content: Some(Align::Center),
        row_gap: Dim::px(12.0),
        ..LayoutStyle::default()
    };
    col
}

fn build(cx: &mut BuildCx) -> Element {
    let note = cx.signal("note", || "hello".to_string());
    let n = note.get(cx.runtime());
    page(vec![
        widgets::text("Editor (see the Preview window)").id("title"),
        widgets::text_field_basic(cx, "note", &n).id("note-input"),
        widgets::button("clear", move |rt| note.set(rt, String::new())).id("clear"),
    ])
}

/// The preview window's own root — same store, different tree.
fn preview(cx: &mut BuildCx) -> Element {
    let note = cx.signal("note", || "hello".to_string());
    page(vec![
        widgets::text("Preview").id("preview-title"),
        widgets::text(format!("» {}", note.get(cx.runtime()))).id("preview-note"),
    ])
}
