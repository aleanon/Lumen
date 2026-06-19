//! pane_grid — a draggable two-pane split styled as a tiny code editor: an
//! Explorer sidebar on the left, a source view on the right. Drag the divider
//! to move the split (the `pane_grid` helper owns the ratio signal). Themed via
//! `app.lss`.
use lumen_widgets::widgets_extra::pane_grid;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Edges};

/// Build the pane-grid app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

fn txt(s: impl Into<String>, size: f32, weight: f32) -> Element {
    let mut e = widgets::text(s);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = size;
        ts.weight = weight;
    }
    e
}

/// A fill column that stretches into its pane slot.
fn pane(children: Vec<Element>) -> Element {
    let mut c = widgets::column(children);
    c.style.width = Dim::pct(1.0);
    c.style.height = Dim::pct(1.0);
    c.style.min_width = Dim::px(0.0); // let the pane flex to the split ratio
    c.style.padding = Edges::all(Dim::px(16.0));
    c.style.row_gap = Dim::px(7.0);
    c
}

fn file(name: &str, active: bool) -> Element {
    let mut e = txt(name, 13.0, if active { 600.0 } else { 400.0 }).class("file");
    if active {
        e = e.class("active");
    }
    e.style.width = Dim::pct(1.0);
    e.style.padding = Edges {
        left: Dim::px(8.0),
        right: Dim::px(8.0),
        top: Dim::px(4.0),
        bottom: Dim::px(4.0),
    };
    e
}

/// One source line: a dim gutter number beside a coloured code fragment.
fn line(n: u32, frag: Element) -> Element {
    let mut g = txt(format!("{n:>2}"), 12.5, 500.0).class("gutter");
    g.style.width = Dim::px(22.0);
    let mut r = widgets::row(vec![g, frag]);
    r.style.column_gap = Dim::px(12.0);
    r.style.align_items = Some(Align::Center);
    r
}

/// A code fragment made of coloured runs laid out in a row.
fn frag(runs: Vec<Element>) -> Element {
    let mut r = widgets::row(runs);
    r.style.column_gap = Dim::px(0.0);
    r
}

fn run(s: &str, class: &str) -> Element {
    txt(s, 13.0, 500.0).class(class)
}

fn build(cx: &mut BuildCx) -> Element {
    let left = pane(vec![
        txt("EXPLORER", 11.0, 800.0).class("section"),
        file("src/", false),
        file("  main.rs", true),
        file("  lib.rs", false),
        file("  theme.rs", false),
        file("Cargo.toml", false),
        file("README.md", false),
    ])
    .id("left");

    let right = pane(vec![
        txt("main.rs", 13.0, 700.0).class("tab"),
        line(1, frag(vec![run("// entry point", "cm")])),
        line(2, frag(vec![run("fn", "kw"), run(" main() {", "code")])),
        line(
            3,
            frag(vec![
                run("    println!(", "code"),
                run("\"hello\"", "str"),
                run(");", "code"),
            ]),
        ),
        line(4, frag(vec![run("}", "code")])),
    ])
    .id("right");

    let split = pane_grid(cx, "split", left, right);

    let header = {
        let mut h = widgets::column(vec![
            txt("Editor", 17.0, 800.0).class("app-title"),
            txt("Drag the divider to resize the panes.", 12.5, 400.0).class("app-sub"),
        ])
        .id("bar");
        h.style.width = Dim::pct(1.0);
        h.style.row_gap = Dim::px(2.0);
        h.style.padding = Edges {
            left: Dim::px(18.0),
            right: Dim::px(18.0),
            top: Dim::px(12.0),
            bottom: Dim::px(12.0),
        };
        h
    };

    let mut page = widgets::column(vec![header, split]).id("page");
    page.style.width = Dim::pct(1.0);
    page.style.height = Dim::pct(1.0);
    page
}
