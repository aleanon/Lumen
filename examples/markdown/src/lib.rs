//! markdown — a "rendered" document: h1/h2 headings, paragraphs, a bullet list,
//! a code block and a blockquote. There is no parser here; the point is the
//! *rendered* look, with each block styled by class from `app.lss`.
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Edges};

/// Build the markdown app.
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

fn h1(s: &str) -> Element {
    txt(s, 28.0, 800.0).class("h1")
}
fn h2(s: &str) -> Element {
    txt(s, 17.0, 700.0).class("h2")
}
/// Body paragraph constrained to the content width so long lines wrap.
const CONTENT_W: f32 = 412.0;

fn p(s: &str) -> Element {
    let mut e = txt(s, 14.0, 400.0).class("p");
    e.style.width = Dim::px(CONTENT_W);
    e
}

/// A bullet item: an accent dot glyph followed by the line text.
fn li(s: &str) -> Element {
    let mut r = widgets::row(vec![
        txt("•", 14.0, 700.0).class("bullet"),
        txt(s, 14.0, 400.0).class("li"),
    ]);
    r.style.column_gap = Dim::px(8.0);
    r
}

fn rule() -> Element {
    let mut e = Element::default().class("rule");
    e.style.height = Dim::px(2.0);
    e.style.width = Dim::pct(1.0);
    e
}

/// A fenced code block: light text on a dark rounded panel.
fn code(s: &str) -> Element {
    let mut e = txt(s, 13.0, 500.0).class("code");
    e.style.width = Dim::pct(1.0);
    e.style.padding = Edges::all(Dim::px(14.0));
    e
}

/// A blockquote: an accent bar beside muted italic-feeling text.
fn quote(s: &str) -> Element {
    let mut bar = Element::default().class("quote-bar");
    bar.style.width = Dim::px(4.0);
    bar.style.align_self = Some(Align::Stretch);

    let mut text = txt(s, 14.0, 400.0).class("quote-text");
    text.style.width = Dim::px(CONTENT_W - 40.0);
    text.style.padding = Edges {
        left: Dim::px(0.0),
        right: Dim::px(4.0),
        top: Dim::px(2.0),
        bottom: Dim::px(2.0),
    };

    let mut r = widgets::row(vec![bar, text]).class("quote");
    r.style.column_gap = Dim::px(12.0);
    r.style.align_items = Some(Align::Stretch);
    r.style.padding = Edges::all(Dim::px(12.0));
    r.style.width = Dim::pct(1.0);
    r
}

fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;
    let mut doc = widgets::column(vec![
        h1("Lumen"),
        p("An AI-first GUI framework with a deterministic CPU renderer as the golden contract."),
        rule(),
        h2("Getting started"),
        p("Add the crate and describe your UI as a tree of elements:"),
        code("let app = App::new(build)\n    .stylesheet(include_str!(\"app.lss\"));"),
        h2("Why a CPU renderer?"),
        li("Deterministic, pixel-exact output for tests."),
        li("Runs headless anywhere — no GPU required."),
        li("The GPU backend is checked against it."),
        quote("\"The renderer is the contract; everything else is an optimisation.\""),
    ])
    .id("card");
    doc.style.row_gap = Dim::px(14.0);
    doc.style.padding = Edges::all(Dim::px(34.0));
    doc.style.width = Dim::px(480.0);
    doc.style.align_items = Some(Align::Stretch);
    doc.shadow = Some(Shadow::soft());

    let mut page = widgets::column(vec![doc]).id("page");
    page.style.width = Dim::pct(1.0);
    page.style.height = Dim::pct(1.0);
    page.style.align_items = Some(Align::Center);
    page.style.justify_content = Some(Align::Center);
    page
}
