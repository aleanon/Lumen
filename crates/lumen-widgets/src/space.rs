//! [`Space`] — empty space between elements. Its `Element` is built inside the
//! constructors: [`Space::new`] (flexible — pushes siblings apart) or
//! [`Space::px`] (a fixed gap).

use crate::widget::impl_common;
use crate::Element;
use lumen_core::semantics::Role;
use lumen_layout::{Dim, LayoutStyle};

/// Empty layout space. Flexible by default (grows to fill the main axis);
/// [`px`](Space::px) makes it a fixed size.
/// # Example
///
/// ```
/// use lumen_widgets::{widgets, App, Space};
///
/// // Space pushes siblings apart; shown here between two labels.
/// let app = App::new(|_| {
///     widgets::row(vec![widgets::text("left"), Space::horizontal(60.0).into(), widgets::text("right")])
/// });
/// # lumen_widgets::doc_shot(app, 200.0, 36.0, "space");
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
    el: Element,
}

impl Space {
    /// Flexible space that grows to push its siblings apart (`flex-grow: 1`).
    pub fn new() -> Space {
        let el = Element {
            role: Role::Generic,
            elide_semantics: true,
            style: LayoutStyle {
                flex_grow: 1.0,
                ..LayoutStyle::default()
            },
            ..Element::default()
        };
        Space { el }
    }

    /// A fixed `w`×`h` gap.
    pub fn px(w: f32, h: f32) -> Space {
        let el = Element {
            role: Role::Generic,
            elide_semantics: true,
            style: LayoutStyle {
                width: Dim::px(w),
                height: Dim::px(h),
                ..LayoutStyle::default()
            },
            ..Element::default()
        };
        Space { el }
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

impl_common!(Space);
