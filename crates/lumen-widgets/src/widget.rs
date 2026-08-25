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
pub use lumen_core::{Color, StableId};
pub use lumen_layout::LayoutStyle;
/// The typed inline `.lss` style, re-exported so [`impl_widget`]'s expansion
/// resolves in a crate that depends only on `lumen-widgets`.
pub use lumen_style::Style;

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
/// This record sits in *every* widget, so its shape is the whole "carry only
/// what you need" question in miniature, and it was got wrong once already.
///
/// Deferring lowering creates a tension that the eager model never faced. An
/// eager `.style(s)` writes into the `LayoutStyle` the node already owns: no
/// allocation, no extra bytes. A deferred `.style(s)` has nowhere to put it
/// yet, so it must either **carry the field inline** — 256 bytes on every
/// widget, including the overwhelming majority that never set it — or **box
/// it**, trading those bytes for one allocation on the few that do.
///
/// Boxing is the right side of that trade here, but only per field. An earlier
/// version boxed all three escape hatches together behind one
/// `Option<Box<Rare>>` with `LayoutStyle` and `Style` inlined *inside* it. That
/// record is ~1.3 KB, so a widget setting a single `.class("x")` allocated 1.3
/// KB to store a string — measured at **+2 allocations and +1.35 MB per 1000
/// widgets** against the eager model. The indirection was added to chase a
/// `Card`/`Chip` regression that later drift control showed was inside the
/// noise floor; it was a fix for a measurement artifact, and it cost more than
/// the artifact did.
///
/// So: `classes` inline (an empty `Vec` allocates nothing), `id` inline
/// (a `StableId` is a `SmolStr`, and short ids never leave the stack — and
/// `.id()` is the one modifier real apps use constantly, since the agent, the
/// tests and `.lss` all address widgets by it), and one pointer each for the
/// two large, rarely-set fields.
#[derive(Default)]
pub struct Common {
    /// Stable id (`.id("…")`) — tests, the agent, focus, `.lss` selectors.
    /// Inline: a short id lives in the `SmolStr` itself, so this allocates
    /// nothing in the case that matters.
    pub(crate) id: Option<StableId>,
    /// `.lss` classes. Inline: an empty `Vec` is three words and no allocation,
    /// and a widget that sets a class pays exactly what the eager model paid.
    pub(crate) classes: Vec<String>,
    /// Background override. `Copy`, so no drop glue.
    pub(crate) background: Option<Color>,
    /// Wholesale layout-style replacement. Boxed: 256 bytes, almost never set.
    pub(crate) style: Option<Box<LayoutStyle>>,
    /// Typed inline `.lss` style (B.6b, `Origin::Inline`). Boxed for the same
    /// reason, and it is larger still.
    pub(crate) css: Option<Box<lumen_style::Style>>,
    /// Disabled: inert *and* dimmed.
    pub(crate) disabled: bool,
}

impl Common {
    /// Set the stable id.
    pub fn set_id(&mut self, id: impl Into<StableId>) {
        self.id = Some(id.into());
    }
    /// Append a `.lss` class.
    pub fn push_class(&mut self, c: impl Into<String>) {
        self.classes.push(c.into());
    }
    /// Override the background fill.
    pub fn set_background(&mut self, c: Color) {
        self.background = Some(c);
    }
    /// Replace the layout style wholesale.
    pub fn set_style(&mut self, s: LayoutStyle) {
        self.style = Some(Box::new(s));
    }
    /// Apply a typed inline `.lss` style.
    pub fn set_css(&mut self, s: Style) {
        self.css = Some(Box::new(s));
    }
    /// Mark the widget disabled (dimmed and inert once it lowers).
    pub fn set_disabled(&mut self, yes: bool) {
        self.disabled = yes;
    }

    /// Fold the universal modifiers onto a freshly built element.
    ///
    /// Applied **after** the widget's own construction, which is what makes the
    /// modifiers order-independent: `.disabled(true).ghost()` and
    /// `.ghost().disabled(true)` now produce the same node, because the dimming
    /// is applied here, to whatever the final fill turned out to be.
    pub fn apply(self, el: &mut Element) {
        // Destructured up front so each field is moved or discarded as a value
        // the optimizer can see through, rather than left behind for drop glue.
        let Common {
            id,
            classes,
            background,
            style,
            css,
            disabled,
        } = self;
        if let Some(id) = id {
            el.id = Some(id);
        }
        if !classes.is_empty() {
            // Move the vector rather than `extend` into a fresh one. Most
            // widgets set no class of their own, and `extend` on an empty
            // `Vec` allocates a *second* buffer to copy into — which showed up
            // as a whole extra allocation per widget against the eager model,
            // for no reason but the shape of the code.
            if el.classes.is_empty() {
                el.classes = classes;
            } else {
                el.classes.extend(classes);
            }
        }
        if let Some(bg) = background {
            el.background = Some(bg);
        }
        if let Some(s) = style {
            el.style = *s;
        }
        if let Some(s) = css {
            el.css_inline = Some(s);
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
///
/// **Exported**, because otherwise the trait buys a third party very little: a
/// foreign widget could implement [`Widget`] but would inherit none of `.id()`,
/// `.class()`, `.background()`, `.style()`, `.css()` or `.disabled()`, and
/// would have to hand-write all six plus the disabled dimming to sit beside a
/// built-in on equal terms. Every path in the expansion goes through `$crate`,
/// so a crate depending on nothing but `lumen-widgets` can use it:
///
/// ```
/// use lumen_widgets::{impl_widget, Common, Element, Label, Widget};
///
/// pub struct Stat { caption: String, common: Common }
///
/// impl Stat {
///     pub fn new(caption: &str) -> Stat {
///         Stat { caption: caption.to_string(), common: Common::default() }
///     }
/// }
///
/// impl Widget for Stat {
///     fn build(self) -> Element {
///         let Stat { caption, common } = self;
///         let mut el: Element = Label::new(caption).into();
///         common.apply(&mut el);   // the universal modifiers land here
///         el
///     }
/// }
///
/// impl_widget!(Stat);
///
/// // …and now the foreign widget has the whole shared vocabulary:
/// let el: Element = Stat::new("Requests").id("stat").class("kpi").disabled(true).into();
/// assert!(el.disabled);
/// ```
#[macro_export]
macro_rules! impl_widget {
    ($t:ty) => {
        impl $t {
            /// Set the stable id (tests, the agent, focus, and `.lss` styling).
            pub fn id(mut self, id: impl Into<$crate::widget::StableId>) -> Self {
                self.common.set_id(id);
                self
            }
            /// Add a class (for `.lss` selectors).
            pub fn class(mut self, c: impl Into<String>) -> Self {
                self.common.push_class(c);
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
                self.common.set_disabled(yes);
                self
            }
            /// Override the background fill.
            pub fn background(mut self, color: $crate::widget::Color) -> Self {
                self.common.set_background(color);
                self
            }
            /// Replace the layout style wholesale.
            pub fn style(mut self, s: $crate::widget::LayoutStyle) -> Self {
                self.common.set_style(s);
                self
            }
            /// Apply a typed inline `.lss` style (B.6b, `Origin::Inline`).
            pub fn css(mut self, s: $crate::widget::Style) -> Self {
                self.common.set_css(s);
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

// `#[macro_export]` lands the macro at the crate root; this alias keeps the
// in-crate `use crate::widget::impl_widget;` imports resolving.
pub use crate::impl_widget;

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
