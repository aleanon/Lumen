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
            /// Disable the widget: it stops responding to clicks, hover,
            /// drags, keyboard focus and the agent's `input.invokeAction`,
            /// drops the actions it advertises, reports `SemState::Disabled`
            /// so `:disabled` styling and assistive tech agree with what the
            /// user can actually do — and *looks* it.
            ///
            /// The dimming matters as much as the enforcement: before it, a
            /// disabled button rendered identically to an enabled one, so the
            /// only way to discover it was inert was to click it and watch
            /// nothing happen. A `:disabled` rule in `.lss` still wins over
            /// this default.
            pub fn disabled(mut self, yes: bool) -> Self {
                self.el.disabled = yes;
                if yes {
                    $crate::widget::mute(&mut self.el);
                }
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

/// Wash a disabled subtree out toward the page.
///
/// Applied to the built element and its descendants, so a button's label fades
/// with its fill rather than staying full-strength on a pale box. Blending
/// toward white assumes a light surface, which is the framework's default
/// theme; a `.lss` `:disabled` rule overrides it wherever that is wrong.
pub(crate) fn mute(el: &mut crate::Element) {
    /// How much of the original colour survives.
    const KEEP: f32 = 0.38;
    fn wash(c: lumen_core::Color) -> lumen_core::Color {
        lumen_core::Color::new_linear(
            c.r * KEEP + (1.0 - KEEP),
            c.g * KEEP + (1.0 - KEEP),
            c.b * KEEP + (1.0 - KEEP),
            c.a,
        )
    }
    // A disabled control must not advertise a hand or an I-beam: the pointer
    // shape is a promise about what a click will do, and the answer is nothing.
    el.cursor = None;
    if let Some(bg) = el.background {
        el.background = Some(wash(bg));
    }
    if let Some(b) = &mut el.border {
        b.color = wash(b.color);
    }
    if let Some(ts) = el.text_style_mut() {
        ts.color = wash(ts.color);
    }
    for c in &mut el.children {
        mute(c);
    }
}
