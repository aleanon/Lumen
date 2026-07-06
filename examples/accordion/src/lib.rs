//! accordion — a small FAQ page built from the [`Accordion`] disclosure widget.
//!
//! Three collapsible sections (shipping / returns / support). The first starts
//! open; the other two start collapsed. Each section's clickable header carries a
//! stable `#faq-*` id so the live agent can click it by selector to expand it.
//! `just run accordion` opens the window; `just run-agent accordion` adds the
//! JSON-RPC agent port.
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};
use lumen_widgets::{widgets, Accordion, App, BuildCx, Element};

/// Build the accordion FAQ app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

/// A body line of wrapped text at a fixed width so long copy flows onto multiple
/// lines instead of overflowing the card.
fn line(s: impl Into<String>) -> Element {
    let mut e = widgets::text(s).class("body");
    e.style.width = Dim::px(360.0);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = 14.0;
    }
    e
}

/// A heading line (the page title).
fn heading(s: impl Into<String>) -> Element {
    let mut e = widgets::text(s).class("title");
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = 26.0;
        ts.weight = 800.0;
    }
    e
}

/// One FAQ section: an [`Accordion`] with an id-tagged header and a body of
/// wrapped lines, wrapped in a card container.
fn section(cx: &BuildCx, name: &str, id: &str, title: &str, body: Vec<Element>) -> Element {
    let acc: Element = Accordion::new(cx, name, title)
        .color(Color::WHITE)
        .id(id)
        .body(body)
        .into();
    let mut card = widgets::column(vec![acc]).class("card");
    card.style.padding = Edges::all(Dim::px(18.0));
    card.style.width = Dim::px(400.0);
    card
}

fn build(cx: &mut BuildCx) -> Element {
    // Seed the first section open; the other two default to collapsed. The init
    // closure only runs on first creation, so `Accordion::new`'s `|| false` is a
    // no-op once this signal exists.
    cx.signal("faq.shipping", || true);

    let shipping = section(
        cx,
        "faq.shipping",
        "faq-shipping",
        "How long does shipping take?",
        vec![
            line("Domestic orders arrive in 2–4 business days."),
            line("International delivery takes 7–14 business days and includes tracking."),
        ],
    );
    let returns = section(
        cx,
        "faq.returns",
        "faq-returns",
        "What is your return policy?",
        vec![
            line("Returns are accepted within 30 days of delivery."),
            line("Items must be unused and in their original packaging."),
            line("Refunds are issued to the original payment method within a week."),
        ],
    );
    let support = section(
        cx,
        "faq.support",
        "faq-support",
        "How do I contact support?",
        vec![
            line("Email support@example.com any time — we reply within one business day."),
            line("Live chat is available weekdays from 9am to 5pm."),
        ],
    );

    let mut card = widgets::column(vec![
        heading("Frequently asked questions"),
        shipping,
        returns,
        support,
    ])
    .id("card");
    card.style.row_gap = Dim::px(14.0);
    card.style.align_items = Some(Align::Start);
    card.style.padding = Edges::all(Dim::px(28.0));

    Element {
        role: lumen_core::semantics::Role::Group,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Dim::pct(1.0),
            height: Dim::pct(1.0),
            align_items: Some(Align::Center),
            justify_content: Some(Align::Center),
            ..LayoutStyle::default()
        },
        children: vec![card],
        ..Element::default()
    }
    .id("page")
}
