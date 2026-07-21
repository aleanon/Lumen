//! [`Rule`] — a thin divider line. Its `Element` is built inside the
//! constructors [`Rule::horizontal`] / [`Rule::vertical`].

use crate::widget::impl_common;
use crate::Element;
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_layout::{Dim, LayoutStyle};

const THICKNESS: f32 = 1.0;

/// A separator line — full-width (horizontal) or full-height (vertical).
/// # Example
///
/// ```
/// use lumen_widgets::{App, Rule};
///
/// let app = App::new(|_| Rule::horizontal().into());
/// # lumen_widgets::doc_shot(app, 160.0, 20.0, "rule");
/// ```
///
/// Renders:
///
/// ![Rule example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/rule.png)
///
/// The picture above is `src/doc_shots/rule.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Rule {
    el: Element,
}

impl Rule {
    /// A horizontal rule (a `1px` line spanning the available width).
    pub fn horizontal() -> Rule {
        let el = Element {
            role: Role::Generic,
            elide_semantics: true,
            background: Some(Color::srgb8(0xd9, 0xdd, 0xe3, 0xff)),
            style: LayoutStyle {
                width: Dim::pct(1.0),
                height: Dim::px(THICKNESS),
                ..LayoutStyle::default()
            },
            ..Element::default()
        };
        Rule { el }
    }

    /// A vertical rule (a `1px` line spanning the available height).
    pub fn vertical() -> Rule {
        let el = Element {
            role: Role::Generic,
            elide_semantics: true,
            background: Some(Color::srgb8(0xd9, 0xdd, 0xe3, 0xff)),
            style: LayoutStyle {
                width: Dim::px(THICKNESS),
                height: Dim::pct(1.0),
                ..LayoutStyle::default()
            },
            ..Element::default()
        };
        Rule { el }
    }

    /// Set the line thickness in px.
    pub fn thickness(mut self, px: f32) -> Rule {
        // The thin axis is the fixed-px one (height for horizontal, width for
        // vertical); the long axis is `100%`.
        if matches!(self.el.style.height, Dim::Px(_)) {
            self.el.style.height = Dim::px(px);
        } else {
            self.el.style.width = Dim::px(px);
        }
        self
    }
}

impl_common!(Rule);
