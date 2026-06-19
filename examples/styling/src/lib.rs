//! styling — a "design system" showcase, themed **entirely from `app.lss`**.
//!
//! Structure and spacing live in code (Taffy); every colour comes from the
//! stylesheet's `@tokens` + class selectors (`.badge.info`, `.alert.danger`, …).
//! Run `just win-watch styling examples/styling/app.lss` and edit the tokens to
//! see the whole palette restyle live.
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};

/// Build the styling showcase (stylesheet baked in; watch `app.lss` to reload).
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

const STATUSES: &[(&str, &str)] = &[
    ("INFO", "info"),
    ("SUCCESS", "success"),
    ("WARNING", "warn"),
    ("DANGER", "danger"),
    ("NEUTRAL", "neutral"),
];

fn txt(s: impl Into<String>, size: f32, weight: f32) -> Element {
    let mut e = widgets::text(s);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = size;
        ts.weight = weight;
    }
    e
}

fn pad(mut e: Element, h: f32, v: f32) -> Element {
    e.style.padding = Edges {
        left: Dim::px(h),
        right: Dim::px(h),
        top: Dim::px(v),
        bottom: Dim::px(v),
    };
    e
}

fn hrow(children: Vec<Element>, gap: f32) -> Element {
    let mut r = widgets::row(children);
    r.style.column_gap = Dim::px(gap);
    r.style.align_items = Some(Align::Center);
    r
}

fn vcol(children: Vec<Element>, gap: f32) -> Element {
    let mut c = widgets::column(children);
    c.style.row_gap = Dim::px(gap);
    c
}

fn badge(label: &str, status: &str) -> Element {
    pad(
        txt(label, 12.0, 700.0).class("badge").class(status),
        13.0,
        5.0,
    )
}

fn button(label: &str, status: &str) -> Element {
    let mut e = widgets::button(label, |_| {}).class("btn").class(status);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = 15.0;
        ts.weight = 600.0;
    }
    pad(e, 18.0, 10.0)
}

fn alert(title: &str, body: &str, status: &str) -> Element {
    let mut card = vcol(
        vec![
            txt(title, 15.0, 700.0).class("alert-title").class(status),
            txt(body, 13.0, 400.0).class("alert-body"),
        ],
        4.0,
    )
    .class("alert")
    .class(status);
    card.style.flex_grow = 1.0;
    card.style.flex_basis = Dim::px(0.0);
    card.style.min_width = Dim::px(0.0);
    pad(card, 16.0, 14.0)
}

fn swatch(name: &str, hex: &str, status: &str) -> Element {
    let mut chip = Element {
        style: LayoutStyle {
            width: Dim::px(64.0),
            height: Dim::px(40.0),
            ..LayoutStyle::default()
        },
        ..Element::default()
    }
    .class("swatch")
    .class(status);
    chip.corner_radius = 9.0;
    let mut col = vcol(
        vec![
            chip,
            txt(name, 13.0, 600.0).class("swatch-name"),
            txt(hex, 12.0, 400.0).class("swatch-hex"),
        ],
        4.0,
    );
    col.style.align_items = Some(Align::Center);
    col
}

fn section(label: &str, body: Element) -> Element {
    vcol(vec![txt(label, 12.0, 700.0).class("section"), body], 12.0)
}

fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;

    let header = vcol(
        vec![
            txt("Design System", 30.0, 800.0).id("title"),
            txt(
                "A theme of status colours — styled entirely from app.lss.",
                15.0,
                400.0,
            )
            .class("subtitle"),
        ],
        4.0,
    );

    let badges = section(
        "STATUS BADGES",
        hrow(
            STATUSES
                .iter()
                .map(|(l, s)| badge(l, s))
                .collect::<Vec<_>>(),
            8.0,
        ),
    );

    let buttons = section(
        "BUTTONS",
        hrow(
            vec![
                button("Save", "info"),
                button("Confirm", "success"),
                button("Review", "warn"),
                button("Delete", "danger"),
            ],
            10.0,
        ),
    );

    let alerts = section(
        "ALERTS",
        vcol(
            vec![
                hrow(
                    vec![
                        alert("Information", "Heads up — something worth noting.", "info"),
                        alert("Success", "Your changes were saved.", "success"),
                    ],
                    12.0,
                ),
                hrow(
                    vec![
                        alert("Warning", "This may need your attention.", "warn"),
                        alert("Danger", "This action can't be undone.", "danger"),
                    ],
                    12.0,
                ),
            ],
            12.0,
        ),
    );

    let palette = section(
        "PALETTE",
        hrow(
            vec![
                swatch("Info", "#2563eb", "info"),
                swatch("Success", "#15a34a", "success"),
                swatch("Warning", "#d97706", "warn"),
                swatch("Danger", "#dc2626", "danger"),
                swatch("Neutral", "#64748b", "neutral"),
            ],
            16.0,
        ),
    );

    let mut panel = vcol(vec![header, badges, buttons, alerts, palette], 26.0).id("panel");
    panel.style.width = Dim::px(680.0);
    panel.style.padding = Edges::all(Dim::px(36.0));
    panel.shadow = Some(Shadow::soft());

    Element {
        role: lumen_core::semantics::Role::Group,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Dim::pct(1.0),
            height: Dim::pct(1.0),
            align_items: Some(Align::Center),
            justify_content: Some(Align::Center),
            padding: Edges::all(Dim::px(28.0)),
            ..LayoutStyle::default()
        },
        children: vec![panel],
        ..Element::default()
    }
    .id("page")
}
