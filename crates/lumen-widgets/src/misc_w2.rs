//! W.2 small widgets: [`Skeleton`], [`Avatar`], [`Pagination`], and the
//! standalone [`Align`] container.

use crate::widget::impl_common;
use crate::{widgets, BuildCx, Element};
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_layout::{Align as LAlign, Dim, Display, Edges, FlexDirection, LayoutStyle};
use std::rc::Rc;

/// A loading placeholder block: a soft grey box that pulses (opacity keyed
/// to the clock) while content loads.
/// # Example
///
/// ```
/// use lumen_widgets::{App, Skeleton};
///
/// let app = App::new(|cx| Skeleton::new(cx, 160.0, 16.0).into());
/// # lumen_widgets::doc_shot(app, 180.0, 36.0, "skeleton");
/// ```
pub struct Skeleton {
    el: Element,
}

impl Skeleton {
    /// A pulsing placeholder of the given size.
    pub fn new(cx: &BuildCx, width: f64, height: f64) -> Skeleton {
        cx.animate();
        let t = cx.now_ms() / 1000.0;
        // 0.55..0.95 alpha pulse.
        let a = 0.75 + 0.20 * (t * 2.2).sin();
        let mut el = Element::default().class("skeleton");
        el.role = Role::Generic;
        el.background = Some(Color::srgb8(0xd7, 0xdb, 0xe1, (a * 255.0) as u8));
        el.corner_radius = 6.0;
        el.style.width = Dim::px(width as f32);
        el.style.height = Dim::px(height as f32);
        Skeleton { el }
    }
}

impl_common!(Skeleton);

/// A round avatar showing the initials of a name over a color hashed from it.
/// # Example
///
/// ```
/// use lumen_widgets::{App, Avatar};
///
/// let app = App::new(|_| Avatar::new("Ada Lovelace", 40.0).into());
/// # lumen_widgets::doc_shot(app, 56.0, 56.0, "avatar");
/// ```
pub struct Avatar {
    el: Element,
}

impl Avatar {
    /// An avatar of `diameter` px for `name` (initials + stable hash color).
    pub fn new(name: &str, diameter: f64) -> Avatar {
        let initials: String = name
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .take(2)
            .collect::<String>()
            .to_uppercase();
        let hash: u32 = name
            .bytes()
            .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
        let palette = [
            Color::srgb8(0x1a, 0x73, 0xe8, 0xff),
            Color::srgb8(0x18, 0x8a, 0x42, 0xff),
            Color::srgb8(0xc9, 0x5b, 0x0b, 0xff),
            Color::srgb8(0x8e, 0x24, 0xaa, 0xff),
            Color::srgb8(0xd3, 0x2f, 0x2f, 0xff),
            Color::srgb8(0x00, 0x83, 0x8f, 0xff),
        ];
        let bg = palette[(hash as usize) % palette.len()];

        let mut text = widgets::text(if initials.is_empty() {
            "?".to_string()
        } else {
            initials
        });
        if let Some(ts) = text.text_style_mut() {
            ts.font_size = (diameter * 0.4) as f32;
            ts.weight = 600.0;
            ts.color = Color::srgb8(0xff, 0xff, 0xff, 0xff);
        }
        let mut el = Element {
            role: Role::Image,
            label: name.to_string(),
            background: Some(bg),
            corner_radius: diameter / 2.0,
            children: vec![text],
            ..Element::default()
        };
        el = el.class("avatar");
        el.style.width = Dim::px(diameter as f32);
        el.style.height = Dim::px(diameter as f32);
        el.style.align_items = Some(LAlign::Center);
        el.style.justify_content = Some(LAlign::Center);
        Avatar { el }
    }
}

impl_common!(Avatar);

/// Page navigation: `‹ 1 2 … n ›`, current page in a signal.
/// # Example
///
/// ```
/// use lumen_widgets::{App, Pagination};
///
/// let app = App::new(|cx| Pagination::new(cx, "page", 5).into());
/// # lumen_widgets::doc_shot(app, 240.0, 48.0, "pagination");
/// ```
pub struct Pagination {
    el: Element,
}

impl Pagination {
    /// A pager over `pages` pages (1-based); the current page lives in the
    /// `{name}.page` signal (`i64`, clamped).
    pub fn new(cx: &BuildCx, name: &str, pages: i64) -> Pagination {
        let pages = pages.max(1);
        let page = cx.signal(&format!("{name}.page"), || 1i64);
        let cur = page.get(cx.runtime()).clamp(1, pages);

        let btn = |label: String, target: i64, active: bool, enabled: bool| {
            let mut b = widgets::text(label);
            if let Some(ts) = b.text_style_mut() {
                ts.font_size = 13.0;
                ts.color = if active {
                    Color::srgb8(0xff, 0xff, 0xff, 0xff)
                } else if enabled {
                    Color::srgb8(0x1c, 0x22, 0x30, 0xff)
                } else {
                    Color::srgb8(0xb3, 0xb9, 0xc2, 0xff)
                };
            }
            b.role = Role::Button;
            b.focusable = enabled;
            b.background = Some(if active {
                crate::theme::accent()
            } else {
                Color::srgb8(0xf3, 0xf5, 0xf8, 0xff)
            });
            b.corner_radius = 6.0;
            b.style.padding = Edges {
                left: Dim::px(9.0),
                right: Dim::px(9.0),
                top: Dim::px(4.0),
                bottom: Dim::px(4.0),
            };
            if enabled {
                b.on_click = Some(Rc::new(move |rt| {
                    page.set(rt, target.clamp(1, pages));
                }));
            }
            b
        };

        let mut children =
            vec![btn("‹".into(), cur - 1, false, cur > 1).id(format!("{name}-prev"))];
        for p in 1..=pages {
            children.push(btn(p.to_string(), p, p == cur, true).id(format!("{name}-p{p}")));
        }
        children.push(btn("›".into(), cur + 1, false, cur < pages).id(format!("{name}-next")));

        let mut row = widgets::row(children).class("pagination");
        row.role = Role::Group;
        row.style.column_gap = Dim::px(6.0);
        row.style.align_items = Some(LAlign::Center);
        Pagination { el: row }
    }
}

impl_common!(Pagination);

/// A standalone alignment container: positions one child inside the
/// available box (the M1 list's `Align`).
/// # Example
///
/// ```
/// use lumen_widgets::{widgets, App, AlignBox};
/// use lumen_layout::Align;
///
/// let app = App::new(|_| AlignBox::new(widgets::text("centered"), Align::Center, Align::Center).into());
/// # lumen_widgets::doc_shot(app, 160.0, 60.0, "align_box");
/// ```
pub struct AlignBox {
    el: Element,
}

impl AlignBox {
    /// Center `child` both ways.
    pub fn center(child: Element) -> AlignBox {
        AlignBox::new(child, LAlign::Center, LAlign::Center)
    }

    /// Explicit cross-axis (`align`) and main-axis (`justify`) placement.
    pub fn new(child: Element, align: LAlign, justify: LAlign) -> AlignBox {
        let el = Element {
            role: Role::Generic,
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                align_items: Some(align),
                justify_content: Some(justify),
                flex_grow: 1.0,
                ..LayoutStyle::default()
            },
            children: vec![child],
            ..Element::default()
        };
        AlignBox { el }
    }
}

impl_common!(AlignBox);
