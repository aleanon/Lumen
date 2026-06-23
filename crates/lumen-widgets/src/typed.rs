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

use crate::{widgets, BuildCx, Element};
use lumen_core::Color;
use lumen_render::RgbaImage;

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

/// A text run. Exposes only typography — size, weight, colour, line-height,
/// letter-spacing — and the universal modifiers. No event handlers.
pub struct Text {
    el: Element,
}

impl Text {
    /// Text labelled `s`.
    pub fn new(s: impl Into<String>) -> Self {
        Text {
            el: widgets::text(s),
        }
    }
    /// Font size in logical px.
    pub fn size(mut self, px: f32) -> Self {
        if let Some(ts) = self.el.text_style_mut() {
            ts.font_size = px;
        }
        self
    }
    /// Font weight (100–900).
    pub fn weight(mut self, w: f32) -> Self {
        if let Some(ts) = self.el.text_style_mut() {
            ts.weight = w;
        }
        self
    }
    /// Bold (weight 700).
    pub fn bold(self) -> Self {
        self.weight(700.0)
    }
    /// Text colour.
    pub fn color(mut self, c: Color) -> Self {
        if let Some(ts) = self.el.text_style_mut() {
            ts.color = c;
        }
        self
    }
    /// Line height as a multiple of font size (B2).
    pub fn line_height(mut self, multiple: f32) -> Self {
        if let Some(ts) = self.el.text_style_mut() {
            ts.line_height = Some(multiple);
        }
        self
    }
    /// Extra letter tracking, px (B2).
    pub fn letter_spacing(mut self, px: f32) -> Self {
        if let Some(ts) = self.el.text_style_mut() {
            ts.letter_spacing = px;
        }
        self
    }
}

styled!(Text);

/// An image at its own pixel size. Universal modifiers only.
pub struct Image {
    el: Element,
}

impl Image {
    /// An image from decoded pixels.
    pub fn new(img: RgbaImage) -> Self {
        Image {
            el: widgets::image(img),
        }
    }
}

styled!(Image);

/// A self-stateful checkbox (state keyed by `name`). Universal modifiers only.
pub struct Checkbox {
    el: Element,
}

impl Checkbox {
    /// A checkbox with `label`, state stored under `name`.
    pub fn new(cx: &BuildCx, name: &str, label: impl Into<String>) -> Self {
        Checkbox {
            el: widgets::checkbox(cx, name, label),
        }
    }
}

styled!(Checkbox);

/// A self-stateful single-line text field (value keyed by `name`).
pub struct TextField {
    el: Element,
}

impl TextField {
    /// A text field with `initial` contents, stored under `name`.
    pub fn new(cx: &BuildCx, name: &str, initial: &str) -> Self {
        TextField {
            el: widgets::text_field_basic(cx, name, initial),
        }
    }
}

styled!(TextField);

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
