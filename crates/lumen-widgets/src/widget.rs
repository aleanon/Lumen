//! Shared plumbing for the typed widgets (`button`, `label`, `text_input`,
//! `slider`, `scrollable`, `container`).
//!
//! Each widget lives in its own file as a newtype wrapping an [`Element`] that it
//! builds **inside its `::new()`**, then exposes only the modifiers relevant to
//! it. The `impl_common!` macro adds the universal modifiers (`id`, `class`,
//! `background`, `style`) and the `From<W> for Element` lowering, so the engine
//! still consumes the same flat `Element` (uniform/SoA pipeline, determinism, and
//! `Clone`/inspectability intact).
//!
//! [`Element`]: crate::Element

/// Implement the universal widget modifiers + `From<W> for Element` for a newtype
/// that stores its element in a field named `el`.
macro_rules! impl_common {
    ($t:ty) => {
        impl $t {
            /// Set the stable id (tests, the agent, focus, and `.lss` styling).
            pub fn id(mut self, id: impl Into<lumen_core::StableId>) -> Self {
                self.el = self.el.id(id);
                self
            }
            /// Add a class (for `.lss` selectors).
            pub fn class(mut self, c: impl Into<String>) -> Self {
                self.el = self.el.class(c);
                self
            }
            /// Override the background fill.
            pub fn background(mut self, color: lumen_core::Color) -> Self {
                self.el.background = Some(color);
                self
            }
            /// Replace the layout style wholesale.
            pub fn style(mut self, s: lumen_layout::LayoutStyle) -> Self {
                self.el = self.el.style(s);
                self
            }
            /// Apply a typed inline `.lss` style (B.6b, `Origin::Inline`).
            pub fn css(mut self, s: lumen_style::Style) -> Self {
                self.el = self.el.css(s);
                self
            }
            /// Borrow the built element (inspection/tests).
            pub fn element(&self) -> &$crate::Element {
                &self.el
            }
            /// Mutably borrow the built element (escape hatch for one-off layout
            /// tweaks not covered by a dedicated modifier).
            pub fn element_mut(&mut self) -> &mut $crate::Element {
                &mut self.el
            }
        }
        impl From<$t> for $crate::Element {
            fn from(w: $t) -> $crate::Element {
                w.el
            }
        }
    };
}

pub(crate) use impl_common;
