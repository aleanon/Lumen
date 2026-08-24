//! The [`Widget`] trait and the shared plumbing behind the typed widgets.
//!
//! # The model
//!
//! A typed widget is a **record of intent**, not a node. `Button::new("Save")`
//! stores a label, a variant and (later) a handler; it does not allocate an
//! [`Element`]. The node is materialized exactly once, at the moment the widget
//! is handed to the tree, by [`Widget::build`].
//!
//! This replaces the earlier model, in which every widget was a newtype over an
//! `Element` built inside its `::new()` and mutated by each modifier. That model
//! was simple and it worked, but it had three costs:
//!
//! * **Every widget was `size_of::<Element>()` — 1072 bytes.** A builder chain
//!   takes `self` by value and returns `Self`, so `Button::new(..).ghost()
//!   .on_press(..).id(..)` nominally moves a kilobyte per link.
//! * **Modifiers paid to undo `new()`'s work.** `Button::new` writes an accent
//!   background; `.ghost()` overwrites it. `Label::size().bold().color()`
//!   re-matched `NodeContent` on every call to reach the `TextStyle`.
//! * **Order mattered where it should not.** `.disabled(true)` applied the
//!   dimming *immediately*, so a later `.ghost()` silently un-dimmed the button.
//!
//! Deferred lowering fixes all three: the widget carries only its own fields,
//! modifiers are plain field writes, and the universal modifiers in [`Common`]
//! are folded on *after* the widget builds — so `disabled` always wins.
//!
//! # What did not change
//!
//! [`Element`] is still what the engine consumes: the uniform SoA pipeline,
//! determinism, `Clone`, and inspectability are untouched. `From<W> for Element`
//! still exists (it is now a call to [`Widget::build`]), so every existing call
//! site — `Button::new("x").into()` — compiles unchanged.

use crate::Element;
use lumen_core::{Color, StableId};
use lumen_layout::LayoutStyle;

/// A widget: data now, [`Element`] later.
///
/// Implementors store only the fields they need and materialize the node in
/// [`build`](Widget::build). Taking `self` by value is deliberate — it lets a
/// widget *move* its `String`s and `Rc`s into the node instead of cloning them.
pub trait Widget: Sized {
    /// Lower this widget's data into the flat [`Element`] the engine consumes.
    ///
    /// Implementations must fold their [`Common`] on last (via
    /// [`Common::apply`]), so the universal modifiers override the widget's own
    /// defaults and `disabled` dimming lands on the finished node.
    fn build(self) -> Element;
}

/// The universal modifiers every typed widget supports, held as data.
///
/// # Why it is shaped like this
///
/// This record sits in *every* widget, so its cost is paid by every widget —
/// including the many that never touch a universal modifier at all. The first
/// cut held all six fields inline (a `Vec<String>`, an `Option<StableId>` and
/// two `Option<Box<_>>`, ~88 bytes) and measurably *lost* to the eager model on
/// widgets built without modifiers: `Card` and `Chip` regressed ~8%. The cost
/// was not the bytes but the **drop glue** — a `Vec` and two `Box`es have to be
/// checked and freed on every widget drop, even when all three are empty.
///
/// So the rarely-set fields are pushed behind a single `Option<Box<Rare>>`:
/// one pointer, one null check to drop. `id` stays inline because a `StableId`
/// is a `SmolStr` that keeps short ids (nearly all of them) on the stack, and
/// `.id()` is the one universal modifier real apps reach for constantly — the
/// agent, the tests and `.lss` all address widgets by it.
#[derive(Default)]
pub struct Common {
    /// Stable id (`.id("…")`) — tests, the agent, focus, `.lss` selectors.
    /// Inline: a short id lives in the `SmolStr` itself, so this allocates
    /// nothing in the case that matters.
    pub(crate) id: Option<StableId>,
    /// Background override. `Copy`, so no drop glue.
    pub(crate) background: Option<Color>,
    /// Disabled: inert *and* dimmed.
    pub(crate) disabled: bool,
    /// The escape hatches, allocated only if one is used.
    pub(crate) rare: Option<Box<Rare>>,
}

/// The universal modifiers that are almost never set, boxed as a unit.
#[derive(Default)]
pub struct Rare {
    /// `.lss` classes.
    pub(crate) classes: Vec<String>,
    /// Wholesale layout-style replacement.
    pub(crate) style: Option<LayoutStyle>,
    /// Typed inline `.lss` style (B.6b, `Origin::Inline`).
    pub(crate) css: Option<lumen_style::Style>,
}

impl Common {
    /// The boxed escape-hatch record, created on first use.
    pub(crate) fn rare(&mut self) -> &mut Rare {
        self.rare.get_or_insert_with(Default::default)
    }

    /// Fold the universal modifiers onto a freshly built element.
    ///
    /// Applied **after** the widget's own construction, which is what makes the
    /// modifiers order-independent: `.disabled(true).ghost()` and
    /// `.ghost().disabled(true)` now produce the same node, because the dimming
    /// is applied here, to whatever the final fill turned out to be.
    pub(crate) fn apply(self, el: &mut Element) {
        // Destructured up front so each field is moved or discarded as a value
        // the optimizer can see through, rather than left behind for drop glue.
        let Common {
            id,
            background,
            disabled,
            rare,
        } = self;
        if let Some(id) = id {
            el.id = Some(id);
        }
        if let Some(bg) = background {
            el.background = Some(bg);
        }
        if let Some(rare) = rare {
            let Rare { classes, style, css } = *rare;
            if !classes.is_empty() {
                el.classes.extend(classes);
            }
            if let Some(s) = style {
                el.style = s;
            }
            if let Some(s) = css {
                el.css_inline = Some(Box::new(s));
            }
        }
        if disabled {
            el.disabled = true;
            mute(el);
        }
    }
}

/// Implement the universal modifiers + `From<W> for Element` for a widget that
/// stores its [`Common`] in a field named `common`.
///
/// The modifiers write into `self.common` — plain field stores on a small
/// struct, where the previous macro mutated a fully built `Element`.
macro_rules! impl_widget {
    ($t:ty) => {
        impl $t {
            /// Set the stable id (tests, the agent, focus, and `.lss` styling).
            pub fn id(mut self, id: impl Into<lumen_core::StableId>) -> Self {
                self.common.id = Some(id.into());
                self
            }
            /// Add a class (for `.lss` selectors).
            pub fn class(mut self, c: impl Into<String>) -> Self {
                self.common.rare().classes.push(c.into());
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
            ///
            /// The dimming is applied when the widget lowers, not when this is
            /// called, so it no longer matters whether `.disabled(true)` comes
            /// before or after a modifier that sets the fill.
            pub fn disabled(mut self, yes: bool) -> Self {
                self.common.disabled = yes;
                self
            }
            /// Override the background fill.
            pub fn background(mut self, color: lumen_core::Color) -> Self {
                self.common.background = Some(color);
                self
            }
            /// Replace the layout style wholesale.
            pub fn style(mut self, s: lumen_layout::LayoutStyle) -> Self {
                self.common.rare().style = Some(s);
                self
            }
            /// Apply a typed inline `.lss` style (B.6b, `Origin::Inline`).
            pub fn css(mut self, s: lumen_style::Style) -> Self {
                self.common.rare().css = Some(s);
                self
            }
            /// Lower to the flat [`Element`](crate::Element) the engine consumes.
            ///
            /// The inherent twin of [`Widget::build`](crate::Widget::build), so
            /// callers need not import the trait.
            pub fn into_element(self) -> $crate::Element {
                <$t as $crate::widget::Widget>::build(self)
            }
        }
        impl From<$t> for $crate::Element {
            fn from(w: $t) -> $crate::Element {
                <$t as $crate::widget::Widget>::build(w)
            }
        }
    };
}

pub(crate) use impl_widget;

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

// --- transitional -----------------------------------------------------------

/// The **previous** model's macro: universal modifiers that mutate an already
/// built `Element` stored in a field named `el`.
///
/// Kept only while the widget catalogue is migrated to [`Widget`] one file at a
/// time, so the crate compiles at every step. Every remaining user of this is a
/// widget that has not been converted yet; when the last one goes, so does this.
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
            /// Lower to the flat [`Element`](crate::Element) the engine consumes.
            pub fn into_element(self) -> $crate::Element {
                self.el
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
