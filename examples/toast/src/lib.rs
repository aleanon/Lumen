//! toast — a stack of severity-themed notifications (info/success/warn/danger),
//! each a tinted card with a coloured accent bar, title and body. Themed via
//! `app.lss`; the four severities share one layout, differentiated by class.
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Edges};

/// Build the toast app.
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

/// One notification: a coloured accent bar beside a title/body column.
fn toast(kind: &str, title: &str, body: &str) -> Element {
    let mut bar = Element::default().class("bar").class(kind);
    bar.style.width = Dim::px(5.0);
    bar.style.align_self = Some(Align::Stretch);

    let mut col = widgets::column(vec![
        txt(title, 15.0, 700.0).class("t-title"),
        txt(body, 13.0, 400.0).class("t-body"),
    ]);
    col.style.row_gap = Dim::px(3.0);

    let mut row = widgets::row(vec![bar, col]).class("toast").class(kind);
    row.style.column_gap = Dim::px(14.0);
    row.style.padding = Edges {
        left: Dim::px(14.0),
        right: Dim::px(18.0),
        top: Dim::px(13.0),
        bottom: Dim::px(13.0),
    };
    row.style.align_items = Some(Align::Stretch);
    row.style.width = Dim::px(360.0);
    row
}

fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;
    let stack = {
        let mut c = widgets::column(vec![
            toast("info", "Heads up", "A new version is available to install."),
            toast(
                "success",
                "Saved",
                "Your changes were written successfully.",
            ),
            toast(
                "warn",
                "Low disk space",
                "Only 1.2 GB remaining on this volume.",
            ),
            toast(
                "danger",
                "Upload failed",
                "The connection was reset. Try again.",
            ),
        ]);
        c.style.row_gap = Dim::px(12.0);
        c
    };

    let mut card = widgets::column(vec![
        txt("Notifications", 24.0, 800.0).class("title"),
        txt("Four severities, one layout.", 14.0, 400.0).class("subtitle"),
        stack,
    ])
    .id("card");
    card.style.align_items = Some(Align::Center);
    card.style.row_gap = Dim::px(18.0);
    card.style.padding = Edges::all(Dim::px(30.0));
    card.shadow = Some(Shadow::soft());

    let mut page = widgets::column(vec![card]).id("page");
    page.style.width = Dim::pct(1.0);
    page.style.height = Dim::pct(1.0);
    page.style.align_items = Some(Align::Center);
    page.style.justify_content = Some(Align::Center);
    page
}
