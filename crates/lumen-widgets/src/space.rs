//! [`Space`] — empty space between elements. Its `Element` is built inside the
//! constructors: [`Space::new`] (flexible — pushes siblings apart) or
//! [`Space::px`] (a fixed gap).

use crate::widget::{impl_widget, Common, Widget};
use crate::Element;
use lumen_core::semantics::Role;
use lumen_layout::{Dim, LayoutStyle};

/// Empty layout space. Flexible by default (grows to fill the main axis);
/// [`px`](Space::px) makes it a fixed size.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, widgets, Space, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     // Space pushes siblings apart; shown here between two labels.
///     let row = widgets::row(vec![
///         widgets::text("left"),
///         Space::horizontal(60.0).into(),
///         widgets::text("right"),
///     ]);
///     centered(cx, row)
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 220.0, 52.0, "space");
/// ```
///
/// Renders:
///
/// ![Space example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/space.png)
///
/// The picture above is `src/doc_shots/space.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Space {
    /// `None` = flexible (`flex-grow: 1`); `Some((w, h))` = a fixed gap.
    ///
    /// The whole widget is a tagged pair of floats. Under the eager model this
    /// same information cost a 1072-byte `Element`.
    size: Option<(f32, f32)>,
    common: Common,
}

impl Space {
    /// Flexible space that grows to push its siblings apart (`flex-grow: 1`).
    pub fn new() -> Space {
        Space {
            size: None,
            common: Common::default(),
        }
    }

    /// A fixed `w`×`h` gap.
    pub fn px(w: f32, h: f32) -> Space {
        Space {
            size: Some((w, h)),
            common: Common::default(),
        }
    }

    /// A fixed-height vertical gap (full width).
    pub fn vertical(px: f32) -> Space {
        Space::px(0.0, px)
    }

    /// A fixed-width horizontal gap (full height).
    pub fn horizontal(px: f32) -> Space {
        Space::px(px, 0.0)
    }
}

impl Default for Space {
    fn default() -> Space {
        Space::new()
    }
}

impl Widget for Space {
    fn build(self) -> Element {
        let Space { size, common } = self;
        let style = match size {
            None => LayoutStyle {
                flex_grow: 1.0,
                ..LayoutStyle::default()
            },
            Some((w, h)) => LayoutStyle {
                width: Dim::px(w),
                height: Dim::px(h),
                ..LayoutStyle::default()
            },
        };
        let mut el = Element {
            role: Role::Generic,
            elide_semantics: true,
            style,
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Space);
