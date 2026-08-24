//! [`Rule`] — a thin divider line. Its `Element` is built inside the
//! constructors [`Rule::horizontal`] / [`Rule::vertical`].

use crate::widget::{impl_widget, Common, Widget};
use crate::Element;
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_layout::{Dim, LayoutStyle};

const THICKNESS: f32 = 1.0;

/// A separator line — full-width (horizontal) or full-height (vertical).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{full_width, Rule, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     full_width(cx, Rule::horizontal().into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 200.0, 48.0, "rule");
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
    /// Which way the line runs.
    axis: Axis,
    /// Thickness of the thin axis, px.
    thickness: f32,
    common: Common,
}

/// Which way a [`Rule`] runs.
#[derive(Clone, Copy)]
enum Axis {
    /// Spans the available width; `thickness` is its height.
    Horizontal,
    /// Spans the available height; `thickness` is its width.
    Vertical,
}

impl Rule {
    /// A horizontal rule (a `1px` line spanning the available width).
    pub fn horizontal() -> Rule {
        Rule {
            axis: Axis::Horizontal,
            thickness: THICKNESS,
            common: Common::default(),
        }
    }

    /// A vertical rule (a `1px` line spanning the available height).
    pub fn vertical() -> Rule {
        Rule {
            axis: Axis::Vertical,
            thickness: THICKNESS,
            common: Common::default(),
        }
    }

    /// Set the line thickness in px.
    ///
    /// Which axis that is follows from `axis` rather than from sniffing whether
    /// the built node's height happened to be a `Dim::Px` — the eager version's
    /// test, which a `.style()` override could defeat.
    pub fn thickness(mut self, px: f32) -> Rule {
        self.thickness = px;
        self
    }
}

impl Widget for Rule {
    fn build(self) -> Element {
        let Rule {
            axis,
            thickness,
            common,
        } = self;
        let (width, height) = match axis {
            Axis::Horizontal => (Dim::pct(1.0), Dim::px(thickness)),
            Axis::Vertical => (Dim::px(thickness), Dim::pct(1.0)),
        };
        let mut el = Element {
            role: Role::Generic,
            elide_semantics: true,
            background: Some(Color::srgb8(0xd9, 0xdd, 0xe3, 0xff)),
            style: LayoutStyle {
                width,
                height,
                ..LayoutStyle::default()
            },
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Rule);
