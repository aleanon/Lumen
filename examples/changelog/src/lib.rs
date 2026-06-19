//! changelog — a keep-a-changelog style release list: version headers with
//! dates, then entries each tagged by a pill badge (Added/Fixed/Changed/
//! Removed). Badge colour comes from `app.lss` via `.badge.<kind>`.
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Edges};

/// Build the changelog app.
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

/// A rounded pill badge (uppercase tag) coloured by kind.
fn badge(kind: &str, label: &str) -> Element {
    let mut e = txt(label, 11.0, 800.0).class("badge").class(kind);
    e.style.padding = Edges {
        left: Dim::px(9.0),
        right: Dim::px(9.0),
        top: Dim::px(3.0),
        bottom: Dim::px(3.0),
    };
    e
}

/// One changelog line: a fixed-width badge column beside the entry text.
fn entry(kind: &str, tag: &str, text: &str) -> Element {
    let mut b = badge(kind, tag);
    b.style.width = Dim::px(74.0);

    let mut r = widgets::row(vec![b, txt(text, 14.0, 400.0).class("entry")]);
    r.style.column_gap = Dim::px(12.0);
    r.style.align_items = Some(Align::Center);
    r
}

fn rule() -> Element {
    let mut e = Element::default().class("rule");
    e.style.height = Dim::px(1.0);
    e.style.width = Dim::pct(1.0);
    e
}

/// A version header: bold version number with a muted release date.
fn version(ver: &str, date: &str) -> Element {
    let mut r = widgets::row(vec![
        txt(ver, 17.0, 800.0).class("ver"),
        txt(date, 12.0, 500.0).class("date"),
    ]);
    r.style.column_gap = Dim::px(10.0);
    r.style.align_items = Some(Align::Center);
    r
}

fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;
    let body = {
        let mut c = widgets::column(vec![
            version("1.2.0", "2026-06-17"),
            entry(
                "added",
                "ADDED",
                "Standalone example crates with .lss theming.",
            ),
            entry(
                "added",
                "ADDED",
                "Live reloader and window agent for fine-tuning.",
            ),
            entry(
                "fixed",
                "FIXED",
                "Button text origin no longer clips at the top.",
            ),
            rule(),
            version("1.1.0", "2026-05-02"),
            entry(
                "changed",
                "CHANGED",
                "Runtime is now generic over the renderer.",
            ),
            entry(
                "fixed",
                "FIXED",
                ".lss text colour is applied during paint.",
            ),
            entry("removed", "REMOVED", "The legacy inline-style shim."),
        ]);
        c.style.row_gap = Dim::px(11.0);
        c.style.align_items = Some(Align::Start);
        c
    };

    let mut card = widgets::column(vec![
        txt("Changelog", 24.0, 800.0).class("title"),
        txt("Notable changes, newest first.", 14.0, 400.0).class("subtitle"),
        body,
    ])
    .id("card");
    card.style.row_gap = Dim::px(16.0);
    card.style.padding = Edges::all(Dim::px(32.0));
    card.style.width = Dim::px(480.0);
    card.style.align_items = Some(Align::Start);
    card.shadow = Some(Shadow::soft());

    let mut page = widgets::column(vec![card]).id("page");
    page.style.width = Dim::pct(1.0);
    page.style.height = Dim::pct(1.0);
    page.style.align_items = Some(Align::Center);
    page.style.justify_content = Some(Align::Center);
    page
}
