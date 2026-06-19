//! Typed widget builders — the spec's "ElementBuilder" surface (02 §3).
//!
//! Each widget is a newtype wrapping an [`Element`] that exposes **only** the
//! modifiers relevant to that widget (plus the universal ones), then lowers via
//! `From<W> for Element`. This gives compile-time type safety and per-widget
//! discoverability (a `Button` has no `.on_drag()` or `.letter_spacing()`) with
//! **zero runtime cost** — the engine still consumes the same flat `Element`, so
//! the uniform/SoA pipeline, determinism, and `Clone`/inspectability are intact.
//!
//! Additive: the function constructors in [`crate::widgets`] still work; this is
//! a typed facade layered on top. Mix typed widgets and raw `Element`s freely
//! via the [`col!`](crate::col)/[`row!`](crate::row) macros.

use crate::Element;
use lumen_core::state::Runtime;
use lumen_core::Color;

/// Generate the universal modifiers (`id`/`class`/`background`/`style`) and the
/// `From<W> for Element` lowering for a typed widget that stores its `Element`
/// in a field named `el`.
macro_rules! styled {
    ($t:ty) => {
        impl $t {
            /// Set the stable id (tests, the agent, focus, and `.lss` styling).
            pub fn id(mut self, id: impl Into<lumen_core::StableId>) -> Self {
                self.el = self.el.id(id);
                self
            }
            /// Add a class.
            pub fn class(mut self, c: impl Into<String>) -> Self {
                self.el = self.el.class(c);
                self
            }
            /// Override the background fill.
            pub fn background(mut self, color: lumen_core::Color) -> Self {
                self.el = self.el.background(color);
                self
            }
            /// Replace the layout style.
            pub fn style(mut self, s: lumen_layout::LayoutStyle) -> Self {
                self.el = self.el.style(s);
                self
            }
        }
        impl From<$t> for $crate::Element {
            fn from(w: $t) -> $crate::Element {
                w.el
            }
        }
    };
}

/// A push button. Exposes only button-relevant modifiers — a press handler and
/// visual emphasis — plus the universal ones. It cannot be given text tracking,
/// a drag handler, or other non-button settings: those methods don't exist.
pub struct Button {
    el: Element,
}

impl Button {
    /// A button labelled `label` (accent/primary style by default).
    pub fn new(label: impl Into<String>) -> Self {
        Button {
            el: Element::button(label),
        }
    }

    /// Run `f` when the button is pressed.
    pub fn on_press(mut self, f: impl Fn(&Runtime) + 'static) -> Self {
        self.el = self.el.on_click(f);
        self
    }

    /// Accent (primary) emphasis — the default, but explicit reads clearly.
    pub fn primary(mut self) -> Self {
        self.el.background = Some(Color::srgb8(0x1a, 0x73, 0xe8, 0xff));
        if let Some(ts) = self.el.text_style_mut() {
            ts.color = Color::WHITE;
            ts.weight = 600.0;
        }
        self
    }

    /// Quiet (ghost) emphasis.
    pub fn ghost(mut self) -> Self {
        self.el.background = Some(Color::srgb8(0xe9, 0xeb, 0xef, 0xff));
        if let Some(ts) = self.el.text_style_mut() {
            ts.color = Color::srgb8(0x1f, 0x23, 0x29, 0xff);
            ts.weight = 600.0;
        }
        self
    }
}

styled!(Button);

/// A column of heterogeneous children — typed widgets and/or `Element`s — each
/// lowered via `Into<Element>`: `col![Button::new("ok").primary(), text("hi")]`.
#[macro_export]
macro_rules! col {
    ($($child:expr),* $(,)?) => {
        $crate::widgets::column(::std::vec![$( $crate::Element::from($child) ),*])
    };
}

/// Row counterpart of [`col!`].
#[macro_export]
macro_rules! row {
    ($($child:expr),* $(,)?) => {
        $crate::widgets::row(::std::vec![$( $crate::Element::from($child) ),*])
    };
}
