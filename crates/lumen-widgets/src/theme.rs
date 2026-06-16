//! A small, opinionated modern theme: a soft app surface, rounded cards,
//! headings, and a filled accent button. These are plain [`Element`]
//! constructors (same convention as [`widgets`]), so an app or
//! example gets a consistent, contemporary look without a stylesheet — the
//! colours, padding, and radii are baked into the element fields the renderer
//! already honours.

use crate::element::Element;
use crate::widgets;
use lumen_core::semantics::Role;
use lumen_core::{Color, Runtime};
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};

/// Soft neutral app background (behind the cards).
pub fn bg() -> Color {
    Color::srgb8(0xf4, 0xf5, 0xf7, 0xff)
}
/// Card / raised-surface colour.
pub fn surface() -> Color {
    Color::WHITE
}
/// Primary text colour.
pub fn ink() -> Color {
    Color::srgb8(0x1f, 0x23, 0x29, 0xff)
}
/// Secondary / muted text colour.
pub fn muted() -> Color {
    Color::srgb8(0x6b, 0x72, 0x80, 0xff)
}
/// Accent colour (buttons, highlights).
pub fn accent() -> Color {
    Color::srgb8(0x1a, 0x73, 0xe8, 0xff)
}

/// A bold-looking section heading.
pub fn heading(s: impl Into<String>) -> Element {
    let mut el = widgets::text(s);
    if let Some((_, ts)) = &mut el.text {
        ts.font_size = 22.0;
        ts.color = ink();
    }
    el
}

/// Muted caption / secondary text.
pub fn caption(s: impl Into<String>) -> Element {
    let mut el = widgets::text(s);
    if let Some((_, ts)) = &mut el.text {
        ts.font_size = 13.0;
        ts.color = muted();
    }
    el
}

/// A white, rounded, padded card wrapping `body`. If `body` is a flex container,
/// it is given comfortable gaps so the controls inside don't sit flush together.
pub fn card(mut body: Element) -> Element {
    if !body.children.is_empty() {
        body.style.row_gap = Dim::px(10.0);
        body.style.column_gap = Dim::px(10.0);
    }
    Element {
        role: Role::Group,
        background: Some(surface()),
        corner_radius: 12.0,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            padding: Edges::all(Dim::px(20.0)),
            row_gap: Dim::px(12.0),
            align_items: Some(Align::Start),
            ..LayoutStyle::default()
        },
        children: vec![body],
        ..Element::default()
    }
}

/// A full-window screen: soft background, comfortable padding, a `title`
/// heading, and `body` inside a card. The standard chrome for an example.
pub fn screen(title: &str, body: Element) -> Element {
    Element {
        role: Role::Group,
        background: Some(bg()),
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Dim::pct(1.0),
            height: Dim::pct(1.0),
            padding: Edges::all(Dim::px(24.0)),
            row_gap: Dim::px(16.0),
            align_items: Some(Align::Stretch),
            ..LayoutStyle::default()
        },
        children: vec![heading(title), card(body)],
        ..Element::default()
    }
}

/// A filled accent button (white label on the accent colour, rounded).
pub fn accent_button(label: impl Into<String>, on_click: impl Fn(&Runtime) + 'static) -> Element {
    let mut el = widgets::button(label, on_click);
    el.background = Some(accent());
    el.corner_radius = 8.0;
    el.style.padding = Edges::all(Dim::px(10.0));
    if let Some((_, ts)) = &mut el.text {
        ts.color = Color::WHITE;
    }
    el
}
