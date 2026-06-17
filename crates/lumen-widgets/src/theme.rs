//! A small, opinionated modern theme: a soft app surface, rounded cards with a
//! soft shadow, bold headings, and filled accent buttons. These are plain
//! [`Element`] constructors (same convention as [`widgets`]), so an app or
//! example gets a consistent, contemporary look without a stylesheet — the
//! colours, weights, padding, radii, and shadows are baked into the element
//! fields the renderer already honours.

use crate::element::{Element, Shadow};
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

/// Styled text of `size`/`weight`/`color` (helper for the typography below).
fn styled(s: impl Into<String>, size: f32, weight: f32, color: Color) -> Element {
    let mut el = widgets::text(s);
    if let Some((_, ts)) = &mut el.text {
        ts.font_size = size;
        ts.weight = weight;
        ts.color = color;
    }
    el
}

/// A very large bold display value (e.g. a stopwatch readout).
pub fn display(s: impl Into<String>) -> Element {
    styled(s, 52.0, 700.0, ink())
}

/// A bold section heading.
pub fn heading(s: impl Into<String>) -> Element {
    styled(s, 22.0, 700.0, ink())
}

/// Muted caption / secondary text.
pub fn caption(s: impl Into<String>) -> Element {
    styled(s, 13.0, 400.0, muted())
}

/// A white, rounded, soft-shadowed card wrapping `body`. If `body` is a flex
/// container, it is given comfortable gaps so the controls don't sit flush.
pub fn card(body: Element) -> Element {
    panel(body, Align::Start)
}

/// A card whose content is centred (the standard "rounded square" surface).
pub fn panel_centered(body: Element) -> Element {
    panel(body, Align::Center)
}

fn panel(mut body: Element, align: Align) -> Element {
    if !body.children.is_empty() {
        body.style.row_gap = Dim::px(14.0);
        body.style.column_gap = Dim::px(12.0);
    }
    Element {
        role: Role::Group,
        background: Some(surface()),
        corner_radius: 16.0,
        shadow: Some(Shadow::soft()),
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            padding: Edges::all(Dim::px(28.0)),
            row_gap: Dim::px(16.0),
            align_items: Some(align),
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

/// A full-window screen that centres `body` (both axes) on the soft background —
/// for a single hero surface like the stopwatch.
pub fn center_screen(body: Element) -> Element {
    Element {
        role: Role::Group,
        background: Some(bg()),
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Dim::pct(1.0),
            height: Dim::pct(1.0),
            align_items: Some(Align::Center),
            justify_content: Some(Align::Center),
            ..LayoutStyle::default()
        },
        children: vec![body],
        ..Element::default()
    }
}

/// A filled accent button — generous padding, semibold white label, rounded.
pub fn accent_button(label: impl Into<String>, on_click: impl Fn(&Runtime) + 'static) -> Element {
    button_styled(label, on_click, accent(), Color::WHITE)
}

/// A neutral (secondary) button — light surface, ink label.
pub fn ghost_button(label: impl Into<String>, on_click: impl Fn(&Runtime) + 'static) -> Element {
    button_styled(label, on_click, Color::srgb8(0xe9, 0xeb, 0xef, 0xff), ink())
}

fn button_styled(
    label: impl Into<String>,
    on_click: impl Fn(&Runtime) + 'static,
    bg: Color,
    fg: Color,
) -> Element {
    let mut el = widgets::button(label, on_click);
    el.background = Some(bg);
    el.corner_radius = 10.0;
    el.style.padding = Edges {
        left: Dim::px(20.0),
        right: Dim::px(20.0),
        top: Dim::px(12.0),
        bottom: Dim::px(12.0),
    };
    if let Some((_, ts)) = &mut el.text {
        ts.color = fg;
        ts.weight = 600.0;
        ts.font_size = 16.0;
    }
    el
}
