//! The typed `Style` — the 1:1 Rust mirror of `.lss` properties (04 §8), the
//! `.lss`→typed application path, and computed-value serialization (04 §7).
//!
//! `Style` setters and `.lss` declarations must agree; the `style_parity!`
//! test asserts that. M0/M1 covers the common property subset used by widgets
//! and the gallery; the remaining v1 properties slot in the same way.

use crate::ast::{Unit, Value};
#[cfg(feature = "snapshot")]
use crate::Origin;
use lumen_core::Color;
use lumen_layout::{
    Align, Dim, Display, Edges, FlexDirection, FlexWrap, GridLine, GridTrack, Position,
};
#[cfg(feature = "snapshot")]
use serde_json::{json, Value as Json};
use std::collections::HashMap;

/// A resolved token table (`@tokens` + the active `@theme`), name → value.
pub type Tokens = HashMap<String, Value>;

/// A parsed `shadow:` declaration — `<dx> <dy> [blur] [spread] <color>`
/// (px offsets/radii; the color's alpha sets the strength). The runtime maps
/// this onto the widget `Shadow` at paint time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StyleShadow {
    /// Horizontal offset (px).
    pub dx: f32,
    /// Vertical offset (px).
    pub dy: f32,
    /// Blur radius (px).
    pub blur: f32,
    /// Spread (px).
    pub spread: f32,
    /// Shadow color.
    pub color: Color,
}

/// One per-side border (B.3): `border-(top|right|bottom|left): <w> <color>`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StyleSideBorder {
    /// Stroke width (px).
    pub width: f32,
    /// Stroke color.
    pub color: Color,
}

/// `blend-mode:` values (B.3) — mirrors the renderer's blend set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleBlend {
    /// `normal` (source-over).
    Normal,
    /// `multiply`.
    Multiply,
    /// `screen`.
    Screen,
    /// `overlay`.
    Overlay,
    /// `darken`.
    Darken,
    /// `lighten`.
    Lighten,
}

/// One `transition:` declaration (B.5, 04 §3): which property animates,
/// how long, with what easing, after what delay.
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    /// The transitioned property (`"background"`, `"opacity"`, … or `"all"`).
    pub property: String,
    /// Duration in ms.
    pub duration_ms: f32,
    /// Easing curve.
    pub easing: crate::anim::Easing,
    /// Start delay in ms.
    pub delay_ms: f32,
}

/// One `animation:` declaration (B.5b, 04 §3): which `@keyframes` timeline
/// plays, for how long, how often.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationSpec {
    /// The `@keyframes` name.
    pub name: String,
    /// Duration of one iteration (ms).
    pub duration_ms: f32,
    /// Easing applied within each keyframe segment.
    pub easing: crate::anim::Easing,
    /// Start delay (ms).
    pub delay_ms: f32,
    /// Iteration count; `None` = infinite.
    pub count: Option<f32>,
    /// Reverse direction on every other iteration.
    pub alternate: bool,
}

/// `clip:` values (B.3, 04 §3): whether/how a node clips its subtree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleClip {
    /// No clipping (default).
    None,
    /// Clip to the square bounds.
    Bounds,
    /// Clip to the rounded bounds (the node's border-radius).
    Rounded,
}

/// A parsed `.lss` gradient (B.3) — box-relative; the runtime maps it onto
/// the renderer's absolute-point `Brush` once the node's bounds are known.
#[derive(Clone, Debug, PartialEq)]
pub struct StyleGradient {
    /// `linear-gradient(<angle>, …)` CSS angle in degrees (0 = to top,
    /// 90 = to right; default 180 = to bottom), or `None` for radial.
    pub angle_deg: Option<f32>,
    /// Color stops with offsets in `[0, 1]`.
    pub stops: Vec<(f32, Color)>,
}

/// The typed computed style. Every field is optional (unset ⇒ inherit/default).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    /// `display`.
    pub display: Option<Display>,
    /// `flex-direction`.
    pub flex_direction: Option<FlexDirection>,
    /// `width`.
    pub width: Option<Dim>,
    /// `height`.
    pub height: Option<Dim>,
    /// `gap` (both axes).
    pub gap: Option<Dim>,
    /// MOD4: values of properties registered via
    /// [`register_property`](crate::register_property).
    ///
    /// The framework carries these through the cascade without interpreting
    /// them; whoever registered the name reads the resolved value. `BTreeMap`
    /// so serialization order is stable — the agent's `ui.getStyles` shows
    /// these, and diffable output matters more here than lookup speed on a map
    /// that is almost always empty.
    pub custom: std::collections::BTreeMap<String, Value>,
    /// SD5/PROP1: the mechanical batch — `LayoutStyle` and taffy already
    /// implement every one of these; `apply()` simply never read the parsed
    /// value into the existing field, so the declarations parsed clean and did
    /// nothing.
    ///
    /// `row-gap`/`column-gap` (per-axis; override the `gap` shorthand).
    pub row_gap: Option<Dim>,
    /// See [`row_gap`](Self::row_gap).
    pub column_gap: Option<Dim>,
    /// `justify-content` — main-axis distribution.
    pub justify_content: Option<Align>,
    /// `align-items` — cross-axis alignment of children.
    pub align_items: Option<Align>,
    /// `align-self` — cross-axis alignment of this item, overriding the
    /// parent's `align-items`.
    pub align_self: Option<Align>,
    /// `flex-wrap`.
    pub flex_wrap: Option<FlexWrap>,
    /// `flex-grow`.
    pub flex_grow: Option<f32>,
    /// `flex-shrink`.
    pub flex_shrink: Option<f32>,
    /// `min-width` / `min-height` / `max-width` / `max-height`.
    pub min_width: Option<Dim>,
    /// See [`min_width`](Self::min_width).
    pub min_height: Option<Dim>,
    /// See [`min_width`](Self::min_width).
    pub max_width: Option<Dim>,
    /// See [`min_width`](Self::min_width).
    pub max_height: Option<Dim>,
    /// `padding` (all sides).
    pub padding: Option<Edges>,
    /// `margin` (all sides).
    pub margin: Option<Edges>,
    /// Per-side `padding-(top|right|bottom|left)` (B.3 longhands) —
    /// `[top, right, bottom, left]`, each independently optional; overrides
    /// the whole-side `padding` component-wise.
    pub padding_sides: [Option<f32>; 4],
    /// Per-side `margin-(top|right|bottom|left)` — as `padding_sides`.
    pub margin_sides: [Option<f32>; 4],
    /// `flex-basis` (PROP1).
    pub flex_basis: Option<Dim>,
    /// `align-content` (PROP1).
    pub align_content: Option<Align>,
    /// `aspect-ratio` (PROP1). Accepts a bare number or `w / h`.
    pub aspect_ratio: Option<f32>,
    /// `position` (PROP1).
    pub position: Option<Position>,
    /// `inset` shorthand (PROP1).
    pub inset: Option<Edges>,
    /// `inset-top/right/bottom/left` longhands (PROP1), in that order. Applied
    /// after the shorthand so per-side values win, matching `padding`/`margin`.
    pub inset_sides: [Option<f32>; 4],
    /// `letter-spacing` (PROP1), extra tracking in logical px.
    pub letter_spacing: Option<f32>,
    /// `font-variation` (PROP1) — CSS `font-variation-settings` syntax.
    pub font_variations: Option<String>,
    /// `z-index` (PROP1) — paint order among SIBLINGS. CSS scopes z-index to a
    /// stacking context; the parent is the context here.
    pub z_index: Option<i32>,
    /// `filter: blur(Npx)` (PROP1) — blurs the element's own content. Distinct
    /// from `backdrop-filter`, which blurs what is behind it.
    pub filter_blur: Option<f32>,
    /// `transform` (PROP1) — a 2D affine, already composed from the CSS
    /// function list. Stored as the matrix rather than the source list because
    /// the paint layer wants one `Affine`, and composing at parse time means
    /// the hot path never re-derives it.
    pub transform: Option<kurbo::Affine>,
    /// `transform-origin` (PROP1) as a fraction of the node's box — `(0.5, 0.5)`
    /// is the centre, which is CSS's default.
    pub transform_origin: Option<(f64, f64)>,
    /// `font-features` (PROP1) — CSS `font-feature-settings` syntax.
    pub font_features: Option<String>,
    /// `text-overflow` (PROP1). `Some(true)` = `ellipsis`.
    pub text_ellipsis: Option<bool>,
    /// `text-wrap` (PROP1). `Some(false)` = `nowrap`.
    pub text_wrap: Option<bool>,
    /// `selection-color` (PROP1) — the text-selection highlight.
    pub selection_color: Option<Color>,
    /// `text-decoration` (PROP1) — underline / line-through.
    pub text_decoration: Option<lumen_core::TextDecoration>,
    /// `cursor` (PROP1) — the pointer shape while over this node.
    pub cursor: Option<lumen_core::CursorShape>,
    /// `font-style: italic | normal` (PROP1).
    pub font_italic: Option<bool>,
    /// `text-align` (PROP1) — alignment of wrapped lines within the box.
    pub text_align: Option<lumen_text::TextAlign>,
    /// `font-family` (PROP1) — a family registered via
    /// `TextEngine::register_font`. Unknown names fall back to the bundled
    /// face, matching the Rust-side `TextStyle::family` contract.
    pub font_family: Option<String>,
    /// `grid-template-columns` (PROP1).
    pub grid_template_columns: Option<Vec<GridTrack>>,
    /// `grid-template-rows` (PROP1).
    pub grid_template_rows: Option<Vec<GridTrack>>,
    /// `grid-column` placement (PROP1), as `(start, end)`.
    pub grid_column: Option<(GridLine, GridLine)>,
    /// `grid-row` placement (PROP1), as `(start, end)`.
    pub grid_row: Option<(GridLine, GridLine)>,
    /// `background` color.
    pub background: Option<Color>,
    /// `background: linear-gradient(…)|radial-gradient(…)` (B.3).
    pub background_gradient: Option<StyleGradient>,
    /// `color` (text).
    pub color: Option<Color>,
    /// `border-radius` (uniform; with a multi-value declaration this holds
    /// the top-left radius as the uniform fallback).
    pub border_radius: Option<f32>,
    /// `border-radius` with 2–4 values (B.3), expanded CSS-style to
    /// `[tl, tr, br, bl]`. `None` for single-value declarations.
    pub border_radius_corners: Option<[f32; 4]>,
    /// `opacity`.
    pub opacity: Option<f32>,
    /// `font-size`.
    pub font_size: Option<f32>,
    /// `font-weight`.
    pub font_weight: Option<u16>,
    /// `line-height` (multiple of font size; B.4).
    pub line_height: Option<f32>,
    /// `backdrop-filter: blur(...)` radius in px (glass).
    pub backdrop_blur: Option<f32>,
    /// `backdrop-filter: saturate(...)` multiplier (`1.0` = none).
    pub backdrop_saturate: Option<f32>,
    /// `backdrop-filter: refraction(...)` edge-lens strength in px (Liquid Glass).
    pub backdrop_refraction: Option<f32>,
    /// `backdrop-filter: specular(...)` rim-highlight intensity.
    pub backdrop_specular: Option<f32>,
    /// `blend-mode` (B.3): composites the subtree onto the backdrop.
    pub blend_mode: Option<StyleBlend>,
    /// `animation:` (B.5b) — a `@keyframes` timeline to play on this node.
    pub animation: Option<AnimationSpec>,
    /// `animation-force: true` (B.5b): keep playing under reduced motion.
    pub animation_force: bool,
    /// `transition:` declarations (B.5) — the runtime animates the paint
    /// tier of these (background/color/opacity/border-radius v1) between
    /// computed values on nodes with stable ids.
    pub transitions: Vec<Transition>,
    /// `clip` (B.3): overrides the element's clip flag; `Bounds` ignores the
    /// border-radius, `Rounded` uses it.
    pub clip: Option<StyleClip>,
    /// `visibility` (B.3): `Some(false)` = hidden — the subtree keeps its
    /// layout space but is removed from paint, hit-testing, and semantics.
    pub visibility: Option<bool>,
    /// `shadow` (B.3): single drop shadow. `inset` and comma lists are not
    /// supported yet (an `inset` keyword disables the declaration).
    pub shadow: Option<StyleShadow>,
    /// Per-side `border-(top|right|bottom|left)` (B.3) —
    /// `[top, right, bottom, left]`; painted as straight strips on top of
    /// the box (border-radius is ignored for per-side borders).
    pub border_sides: [Option<StyleSideBorder>; 4],
    /// `border-width` in px (uniform). Also set by the `border` shorthand.
    pub border_width: Option<f32>,
    /// `border-color`. Also set by the `border` shorthand.
    pub border_color: Option<Color>,
}

impl Style {
    /// An empty style.
    pub fn new() -> Style {
        Style::default()
    }

    // --- the typed Rust mirror (04 §8) -------------------------------------

    /// Set `background`.
    pub fn background(mut self, c: Color) -> Self {
        self.background = Some(c);
        self
    }
    /// Set text `color`.
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }
    /// Set `padding` (all sides, px).
    pub fn padding(mut self, px: f32) -> Self {
        self.padding = Some(Edges::all(Dim::px(px)));
        self
    }
    /// Set `border-radius` (px).
    pub fn radius(mut self, px: f32) -> Self {
        self.border_radius = Some(px);
        self
    }
    /// Set `opacity`.
    pub fn opacity(mut self, o: f32) -> Self {
        self.opacity = Some(o);
        self
    }
    /// Set `font-size` (px).
    pub fn font_size(mut self, px: f32) -> Self {
        self.font_size = Some(px);
        self
    }
    /// Set `font-weight`.
    pub fn font_weight(mut self, w: u16) -> Self {
        self.font_weight = Some(w);
        self
    }
    /// Set `width` (px).
    pub fn width(mut self, px: f32) -> Self {
        self.width = Some(Dim::px(px));
        self
    }
    /// Set `gap` (px).
    pub fn gap(mut self, px: f32) -> Self {
        self.gap = Some(Dim::px(px));
        self
    }
    /// Set `display`.
    /// PROP1 typed setters. ADR-016 requires the typed mirror to cover exactly
    /// the applied set, so implementing a `.lss` property means adding its
    /// setter in the same change — the parity test enforces it.
    pub fn row_gap(mut self, px: f32) -> Self {
        self.row_gap = Some(Dim::px(px));
        self
    }

    /// See [`row_gap`](Self::row_gap).
    pub fn column_gap(mut self, px: f32) -> Self {
        self.column_gap = Some(Dim::px(px));
        self
    }

    /// `justify-content`.
    pub fn justify_content(mut self, a: Align) -> Self {
        self.justify_content = Some(a);
        self
    }

    /// `align-items`.
    pub fn align_items(mut self, a: Align) -> Self {
        self.align_items = Some(a);
        self
    }

    /// `align-self`.
    pub fn align_self(mut self, a: Align) -> Self {
        self.align_self = Some(a);
        self
    }

    /// `flex-wrap`.
    pub fn flex_wrap(mut self, w: FlexWrap) -> Self {
        self.flex_wrap = Some(w);
        self
    }

    /// `grid-template-columns` (PROP1).
    pub fn grid_template_columns(mut self, tracks: Vec<GridTrack>) -> Self {
        self.grid_template_columns = Some(tracks);
        self
    }

    /// `grid-template-rows` (PROP1).
    pub fn grid_template_rows(mut self, tracks: Vec<GridTrack>) -> Self {
        self.grid_template_rows = Some(tracks);
        self
    }

    /// `grid-column` placement (PROP1).
    pub fn grid_column(mut self, start: GridLine, end: GridLine) -> Self {
        self.grid_column = Some((start, end));
        self
    }

    /// `grid-row` placement (PROP1).
    pub fn grid_row(mut self, start: GridLine, end: GridLine) -> Self {
        self.grid_row = Some((start, end));
        self
    }

    /// `letter-spacing` (PROP1), extra tracking in logical px.
    pub fn letter_spacing(mut self, px: f32) -> Self {
        self.letter_spacing = Some(px);
        self
    }

    /// `font-variation` (PROP1).
    pub fn font_variations(mut self, settings: impl Into<String>) -> Self {
        self.font_variations = Some(settings.into());
        self
    }

    /// `z-index` (PROP1).
    pub fn z_index(mut self, z: i32) -> Self {
        self.z_index = Some(z);
        self
    }

    /// `filter: blur()` (PROP1).
    pub fn filter_blur(mut self, px: f32) -> Self {
        self.filter_blur = Some(px);
        self
    }

    /// `transform` (PROP1).
    pub fn transform(mut self, t: kurbo::Affine) -> Self {
        self.transform = Some(t);
        self
    }

    /// `transform-origin` (PROP1), as a fraction of the box.
    pub fn transform_origin(mut self, x: f64, y: f64) -> Self {
        self.transform_origin = Some((x, y));
        self
    }

    /// `font-features` (PROP1).
    pub fn font_features(mut self, settings: impl Into<String>) -> Self {
        self.font_features = Some(settings.into());
        self
    }

    /// `text-overflow` (PROP1) — `true` truncates with an ellipsis.
    pub fn text_ellipsis(mut self, yes: bool) -> Self {
        self.text_ellipsis = Some(yes);
        self
    }

    /// `text-wrap` (PROP1) — `false` disables wrapping.
    pub fn text_wrap(mut self, wrap: bool) -> Self {
        self.text_wrap = Some(wrap);
        self
    }

    /// `selection-color` (PROP1).
    pub fn selection_color(mut self, c: Color) -> Self {
        self.selection_color = Some(c);
        self
    }

    /// `text-decoration` (PROP1).
    pub fn text_decoration(mut self, d: lumen_core::TextDecoration) -> Self {
        self.text_decoration = Some(d);
        self
    }

    /// `cursor` (PROP1).
    pub fn cursor(mut self, c: lumen_core::CursorShape) -> Self {
        self.cursor = Some(c);
        self
    }

    /// `font-style` (PROP1).
    pub fn font_italic(mut self, yes: bool) -> Self {
        self.font_italic = Some(yes);
        self
    }

    /// `text-align` (PROP1).
    pub fn text_align(mut self, a: lumen_text::TextAlign) -> Self {
        self.text_align = Some(a);
        self
    }

    /// `font-family` (PROP1). Register the face first with
    /// `TextEngine::register_font`; unknown names fall back to the bundled one.
    pub fn font_family(mut self, name: impl Into<String>) -> Self {
        self.font_family = Some(name.into());
        self
    }

    /// `flex-basis` (PROP1).
    pub fn flex_basis(mut self, d: Dim) -> Self {
        self.flex_basis = Some(d);
        self
    }

    /// `align-content` (PROP1).
    pub fn align_content(mut self, a: Align) -> Self {
        self.align_content = Some(a);
        self
    }

    /// `aspect-ratio` (PROP1) as width ÷ height.
    pub fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.aspect_ratio = Some(ratio);
        self
    }

    /// `position` (PROP1). `Absolute` takes the node out of flow; place it
    /// with [`inset`](Self::inset).
    pub fn position(mut self, p: Position) -> Self {
        self.position = Some(p);
        self
    }

    /// `inset` (PROP1) — the offsets an `Absolute` node is placed by.
    pub fn inset(mut self, e: Edges) -> Self {
        self.inset = Some(e);
        self
    }

    /// `flex-grow`.
    pub fn flex_grow(mut self, n: f32) -> Self {
        self.flex_grow = Some(n);
        self
    }

    /// `flex-shrink`.
    pub fn flex_shrink(mut self, n: f32) -> Self {
        self.flex_shrink = Some(n);
        self
    }

    /// `min-width`.
    pub fn min_width(mut self, px: f32) -> Self {
        self.min_width = Some(Dim::px(px));
        self
    }

    /// `min-height`.
    pub fn min_height(mut self, px: f32) -> Self {
        self.min_height = Some(Dim::px(px));
        self
    }

    /// `max-width`.
    pub fn max_width(mut self, px: f32) -> Self {
        self.max_width = Some(Dim::px(px));
        self
    }

    /// `max-height`.
    pub fn max_height(mut self, px: f32) -> Self {
        self.max_height = Some(Dim::px(px));
        self
    }

    /// `display`.
    pub fn display(mut self, d: Display) -> Self {
        self.display = Some(d);
        self
    }
    /// Set `flex-direction`.
    pub fn flex_direction(mut self, d: FlexDirection) -> Self {
        self.flex_direction = Some(d);
        self
    }
    /// Set `height` (px).
    pub fn height(mut self, px: f32) -> Self {
        self.height = Some(Dim::px(px));
        self
    }
    /// Set `margin` (all sides, px).
    pub fn margin(mut self, px: f32) -> Self {
        self.margin = Some(Edges::all(Dim::px(px)));
        self
    }
    /// Set one padding side (`0..=3` = top/right/bottom/left, px) — the
    /// typed mirror of the `padding-(top|…)` longhands.
    pub fn padding_side(mut self, side: usize, px: f32) -> Self {
        self.padding_sides[side] = Some(px);
        self
    }
    /// Set one margin side (`0..=3` = top/right/bottom/left, px).
    pub fn margin_side(mut self, side: usize, px: f32) -> Self {
        self.margin_sides[side] = Some(px);
        self
    }
    /// Set `line-height` (multiple of font size).
    pub fn line_height(mut self, mult: f32) -> Self {
        self.line_height = Some(mult);
        self
    }
    /// Set the `border` shorthand (`border: <width> <color>`).
    pub fn border(mut self, width_px: f32, color: Color) -> Self {
        self.border_width = Some(width_px);
        self.border_color = Some(color);
        self
    }
    /// Set `border-width` (px).
    pub fn border_width(mut self, px: f32) -> Self {
        self.border_width = Some(px);
        self
    }
    /// Set `border-color`.
    pub fn border_color(mut self, c: Color) -> Self {
        self.border_color = Some(c);
        self
    }
    /// Set `backdrop-filter: blur(<px>)`.
    pub fn backdrop_blur(mut self, px: f32) -> Self {
        self.backdrop_blur = Some(px);
        self
    }
    /// Set `backdrop-filter: saturate(<mult>)`.
    pub fn backdrop_saturate(mut self, mult: f32) -> Self {
        self.backdrop_saturate = Some(mult);
        self
    }
    /// Set `shadow` (`<dx> <dy> [blur] [spread] <color>`).
    pub fn shadow(mut self, sh: StyleShadow) -> Self {
        self.shadow = Some(sh);
        self
    }
    /// Set one border side (`0..=3` = top/right/bottom/left).
    pub fn border_side(mut self, side: usize, width: f32, color: Color) -> Self {
        self.border_sides[side] = Some(StyleSideBorder { width, color });
        self
    }
    /// Set the `animation:` timeline (B.5b).
    pub fn animation(mut self, a: AnimationSpec) -> Self {
        self.animation = Some(a);
        self
    }
    /// Set `animation-force`.
    pub fn animation_force(mut self, on: bool) -> Self {
        self.animation_force = on;
        self
    }
    /// Add a `transition:` declaration (B.5).
    pub fn transition(mut self, t: Transition) -> Self {
        self.transitions.push(t);
        self
    }
    /// Set `blend-mode`.
    pub fn blend_mode(mut self, b: StyleBlend) -> Self {
        self.blend_mode = Some(b);
        self
    }
    /// Set `clip`.
    pub fn clip(mut self, c: StyleClip) -> Self {
        self.clip = Some(c);
        self
    }
    /// Set `visibility` (`false` = hidden).
    pub fn visibility(mut self, visible: bool) -> Self {
        self.visibility = Some(visible);
        self
    }
    /// Set a gradient `background` (the typed mirror of
    /// `linear-gradient(…)`/`radial-gradient(…)`).
    pub fn background_gradient(mut self, g: StyleGradient) -> Self {
        self.background_gradient = Some(g);
        self
    }
    /// Set per-corner `border-radius` (`[tl, tr, br, bl]`, px).
    pub fn radius_corners(mut self, c: [f32; 4]) -> Self {
        self.border_radius_corners = Some(c);
        self.border_radius = Some(c[0]);
        self
    }
}

/// SD5.1: properties the parser ACCEPTS but `apply()` does nothing with.
///
/// These are the framework's worst defect class for its stated audience: an
/// author (or an agent) writes a rule, gets no error, and the rule silently
/// does nothing. A human eventually notices the pixels; an agent cannot see the
/// screen, so it reports success and moves on.
///
/// Listing them explicitly does two things. It lets `apply()` emit `W0107`
/// instead of falling through in silence, and — via `style_parity!`'s
/// `KNOWN == APPLIED ∪ PARSE_ONLY` assertion — it makes the *next* property
/// that lands in the parser without an implementation a **build failure**
/// rather than another silent no-op.
///
/// Entries leave this list by being implemented, never by being deleted.
///
/// Most of these are mechanical: `LayoutStyle`/taffy already implement
/// justify-content, align-*, flex-*, min/max-size, aspect-ratio,
/// position/inset and CSS Grid — `apply()` simply never learned to read the
/// parsed value into the existing field.
pub const PARSE_ONLY_PROPERTIES: &[&str] = &[
    // Layout — the field exists in LayoutStyle; apply() doesn't populate it.
    // Visual — needs render support, not just a field.
    // Typography — needs text-stack plumbing.
];

/// SD5.x: does `value` actually apply to `property`?
///
/// Used at parse time to raise `W0109` for a value the runtime cannot use. The
/// test is the same one the parity suite uses on `APPLIED_PROPERTIES`: apply to
/// a fresh [`Style`] and see whether anything changed. Every successful parse
/// sets a field to `Some(..)`, which differs from the default `None`, so
/// "unchanged" means "not understood" — including for values that happen to
/// equal a default, since `Some(0.0) != None`.
///
/// Returns `true` (i.e. "assume fine") in two cases where the answer is not
/// knowable here:
///
/// * the property is not in [`APPLIED_PROPERTIES`] — `W0107` already covers it,
///   and double-reporting one declaration helps nobody;
/// * the value mentions a `$token`, which is resolved later against a token map
///   this function does not have. Reporting those would fire on every themed
///   stylesheet.
pub fn value_applies(property: &str, value: &Value) -> bool {
    if !APPLIED_PROPERTIES.contains(&property) {
        return true;
    }
    // Deliberately narrow: only a bare KEYWORD is judged. A keyword either is
    // in a property's accepted set or is not, so there is no false-positive
    // risk — which is not true of compound values. `transition: 120ms` is a
    // duration with no property or easing and applies nothing, so the general
    // form of this check would warn about it; but shorthands, lists and
    // functions have enough partial-value nuance that reporting them here
    // would mean rejecting stylesheets that work today. Numeric rejections
    // (`aspect-ratio: 0`) are therefore still silent — the remaining sliver of
    // the value-level hole, and the reason this const is named for what it
    // checks rather than for the hole it closes.
    let judgeable = match value {
        // A keyword either is in a property's accepted set or is not.
        Value::Keyword(_) => true,
        // A bare number, but only where the number is the WHOLE value — see
        // `SCALAR_PROPERTIES`.
        Value::Number(..) => SCALAR_PROPERTIES.contains(&property),
        // A function call, or a list of them, where the function list IS the
        // whole value — see `FUNCTION_VALUED_PROPERTIES`.
        Value::Function(..) => FUNCTION_VALUED_PROPERTIES.contains(&property),
        Value::List(items) => {
            FUNCTION_VALUED_PROPERTIES.contains(&property)
                && items.iter().all(|i| matches!(i, Value::Function(..)))
        }
        _ => false,
    };
    if !judgeable {
        return true;
    }
    let mut probe = Style::new();
    apply(&mut probe, property, value, &Tokens::new());
    probe != Style::new()
}

/// Applied properties whose entire value is a single scalar, so a `Number` that
/// `apply` refuses is unambiguously a rejected value (SD5.x).
///
/// An explicit list rather than "everything that is not a shorthand", because
/// the failure modes are not symmetric: a missing entry costs a warning that
/// does not fire, while a wrongly-included compound property costs a warning on
/// a stylesheet that works. `transition: 120ms` is the case that proves the
/// point — a duration with no property or easing, which applies nothing and yet
/// is legal input the general check flagged. New properties therefore opt IN.
///
/// Most entries here can never warn (`opacity` accepts any number); they are
/// listed because they are single-scalar, not because they reject anything
/// today. `aspect-ratio` is the one that currently does, rejecting zero and
/// negatives that would collapse the node in taffy.
const SCALAR_PROPERTIES: &[&str] = &[
    "z-index",
    "aspect-ratio",
    "opacity",
    "flex-grow",
    "flex-shrink",
    "font-weight",
    "line-height",
];

/// Applied properties whose entire value is a function call (or a list of
/// them), so a function `apply` refuses is unambiguously a rejected value.
///
/// Same opt-in discipline as [`SCALAR_PROPERTIES`], and for the same reason:
/// `background: linear-gradient(...)` is also function-valued but has fallback
/// paths, so judging it here would risk warning about input that works.
///
/// `transform` earns its place because its rejections are silent AND wrong-
/// looking: `translate(50%)` parses, is refused (a percentage is relative to
/// the node's own box, which this layer cannot see), and without a diagnostic
/// the author sees an untransformed node with nothing to read.
const FUNCTION_VALUED_PROPERTIES: &[&str] = &["transform", "filter"];

/// The `.lss` properties `apply` actually consumes — the runtime's applied
/// set, in `apply` arm order. The parity test asserts (a) each entry really
/// changes a `Style`, (b) no other known property does, and (c) the typed
/// mirror covers exactly this set — so this const, `apply`, and the setters
/// cannot drift apart silently (04 §8).
pub const APPLIED_PROPERTIES: &[&str] = &[
    "font-variation",
    "flex-wrap",
    "flex-grow",
    "flex-shrink",
    "justify-content",
    "align-items",
    "align-self",
    "row-gap",
    "column-gap",
    "min-width",
    "min-height",
    "max-width",
    "max-height",
    "display",
    "flex-direction",
    "width",
    "height",
    "gap",
    "padding",
    "margin",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "flex-basis",
    "align-content",
    "aspect-ratio",
    "position",
    "inset",
    "inset-top",
    "inset-right",
    "inset-bottom",
    "inset-left",
    "letter-spacing",
    "font-family",
    "text-align",
    "font-style",
    "cursor",
    "text-decoration",
    "selection-color",
    "text-wrap",
    "text-overflow",
    "font-features",
    "z-index",
    "filter",
    "transform",
    "transform-origin",
    "grid-template-columns",
    "grid-template-rows",
    "grid-column",
    "grid-row",
    "overflow",
    "background",
    "color",
    "border-radius",
    "opacity",
    "font-size",
    "font-weight",
    "line-height",
    "backdrop-filter",
    "shadow",
    "blend-mode",
    "transition",
    "animation",
    "animation-force",
    "clip",
    "visibility",
    "border",
    "border-top",
    "border-right",
    "border-bottom",
    "border-left",
    "border-width",
    "border-color",
];

/// Apply one `.lss` declaration to `style`, resolving `$tokens`. Unknown
/// properties are ignored here (the parser already flagged them E0102).
pub fn apply(style: &mut Style, property: &str, value: &Value, tokens: &Tokens) {
    let v = resolve_token(value, tokens);
    match property {
        "display" => style.display = as_display(&v),
        "flex-direction" => style.flex_direction = as_flex_direction(&v),
        "width" => style.width = as_dim(&v),
        "height" => style.height = as_dim(&v),
        "gap" => style.gap = as_dim(&v),
        // PROP1: the mechanical batch — parse straight into fields taffy
        // already honours.
        "row-gap" => style.row_gap = as_dim(&v),
        "column-gap" => style.column_gap = as_dim(&v),
        "justify-content" => style.justify_content = as_align(&v),
        "align-items" => style.align_items = as_align(&v),
        "align-self" => style.align_self = as_align(&v),
        "flex-wrap" => style.flex_wrap = as_flex_wrap(&v),
        "flex-grow" => style.flex_grow = as_number(&v).map(|n| n as f32),
        "flex-shrink" => style.flex_shrink = as_number(&v).map(|n| n as f32),
        "min-width" => style.min_width = as_dim(&v),
        "min-height" => style.min_height = as_dim(&v),
        "max-width" => style.max_width = as_dim(&v),
        "max-height" => style.max_height = as_dim(&v),
        "padding" => style.padding = as_dim(&v).map(Edges::all),
        "margin" => style.margin = as_dim(&v).map(Edges::all),
        "padding-top" => style.padding_sides[0] = as_px(&v),
        "padding-right" => style.padding_sides[1] = as_px(&v),
        "padding-bottom" => style.padding_sides[2] = as_px(&v),
        "padding-left" => style.padding_sides[3] = as_px(&v),
        "margin-top" => style.margin_sides[0] = as_px(&v),
        "margin-right" => style.margin_sides[1] = as_px(&v),
        "margin-bottom" => style.margin_sides[2] = as_px(&v),
        "margin-left" => style.margin_sides[3] = as_px(&v),
        // PROP1: layout properties whose `LayoutStyle` fields already existed —
        // taffy has implemented them all along; `apply` had simply never
        // learned to read them, so they parsed and were silently discarded.
        "flex-basis" => style.flex_basis = as_dim(&v),
        "align-content" => style.align_content = as_align(&v),
        "aspect-ratio" => style.aspect_ratio = as_aspect_ratio(&v),
        "position" => style.position = as_position(&v),
        "inset" => style.inset = as_dim(&v).map(Edges::all),
        "inset-top" => style.inset_sides[0] = as_px(&v),
        "inset-right" => style.inset_sides[1] = as_px(&v),
        "inset-bottom" => style.inset_sides[2] = as_px(&v),
        "inset-left" => style.inset_sides[3] = as_px(&v),
        // PROP1: typography whose `TextStyle` fields already existed.
        "letter-spacing" => style.letter_spacing = as_px(&v),
        "font-family" => style.font_family = as_font_family(&v),
        "text-align" => style.text_align = as_text_align(&v),
        "font-style" => style.font_italic = as_font_style(&v),
        "cursor" => style.cursor = as_cursor(&v),
        "text-decoration" => style.text_decoration = as_text_decoration(&v),
        "selection-color" => style.selection_color = as_color(&v),
        "text-wrap" => style.text_wrap = as_text_wrap(&v),
        "text-overflow" => style.text_ellipsis = as_text_overflow(&v),
        "font-features" => style.font_features = as_feature_settings(&v),
        "font-variation" => style.font_variations = as_feature_settings(&v),
        "z-index" => style.z_index = as_z_index(&v),
        "filter" => style.filter_blur = as_filter_blur(&v),
        "transform" => style.transform = as_transform(&v),
        "transform-origin" => style.transform_origin = as_transform_origin(&v),
        "grid-template-columns" => style.grid_template_columns = as_grid_tracks(&v),
        "grid-template-rows" => style.grid_template_rows = as_grid_tracks(&v),
        "grid-column" => style.grid_column = as_grid_line_pair(&v),
        "grid-row" => style.grid_row = as_grid_line_pair(&v),
        // PROP1: `overflow` writes the EXISTING `clip` field rather than
        // adding a parallel one — CSS `overflow: hidden` and Lumen's `clip`
        // are the same operation, and two fields racing for one behaviour is
        // how contradictory declarations get silently resolved by arm order.
        "overflow" => {
            if let Some(c) = as_overflow(&v) {
                style.clip = Some(c);
            }
        }
        "background" => match &v {
            Value::Function(name, args)
                if name == "linear-gradient" || name == "radial-gradient" =>
            {
                style.background_gradient = as_gradient(name, args)
            }
            other => style.background = as_color(other),
        },
        "color" => style.color = as_color(&v),
        "border-radius" => match &v {
            // 2–4 values expand CSS-style; `border_radius` keeps the
            // top-left as the uniform fallback (shadow shape uses it).
            Value::List(items) => {
                let px: Vec<f32> = items.iter().filter_map(as_px).collect();
                let c = match px.as_slice() {
                    [a] => Some([*a, *a, *a, *a]),
                    [a, b] => Some([*a, *b, *a, *b]),
                    [a, b, c] => Some([*a, *b, *c, *b]),
                    [a, b, c, d] => Some([*a, *b, *c, *d]),
                    _ => None,
                };
                style.border_radius_corners = c;
                style.border_radius = c.map(|c| c[0]);
            }
            one => style.border_radius = as_px(one),
        },
        "opacity" => style.opacity = as_number(&v).map(|n| n as f32),
        "font-size" => style.font_size = as_px(&v),
        "font-weight" => style.font_weight = as_number(&v).map(|n| n as u16),
        "line-height" => style.line_height = as_number(&v).map(|n| n as f32),
        "backdrop-filter" => apply_backdrop(style, &v),
        "shadow" => style.shadow = as_shadow(&v),
        "transition" => style.transitions = parse_transitions(&v),
        "animation" => style.animation = parse_animation(&v),
        "animation-force" => style.animation_force = matches!(&v, Value::Keyword(k) if k == "true"),
        "blend-mode" => {
            style.blend_mode = match &v {
                Value::Keyword(k) => match k.as_str() {
                    "normal" => Some(StyleBlend::Normal),
                    "multiply" => Some(StyleBlend::Multiply),
                    "screen" => Some(StyleBlend::Screen),
                    "overlay" => Some(StyleBlend::Overlay),
                    "darken" => Some(StyleBlend::Darken),
                    "lighten" => Some(StyleBlend::Lighten),
                    _ => None,
                },
                _ => None,
            }
        }
        "clip" => {
            style.clip = match &v {
                Value::Keyword(k) if k == "none" => Some(StyleClip::None),
                Value::Keyword(k) if k == "bounds" => Some(StyleClip::Bounds),
                Value::Keyword(k) if k == "rounded" => Some(StyleClip::Rounded),
                _ => None,
            }
        }
        "visibility" => {
            style.visibility = match &v {
                Value::Keyword(k) if k == "visible" => Some(true),
                Value::Keyword(k) if k == "hidden" => Some(false),
                _ => None,
            }
        }
        "border" => apply_border(style, &v),
        "border-top" => style.border_sides[0] = as_side_border(&v),
        "border-right" => style.border_sides[1] = as_side_border(&v),
        "border-bottom" => style.border_sides[2] = as_side_border(&v),
        "border-left" => style.border_sides[3] = as_side_border(&v),
        "border-width" => style.border_width = as_px(&v),
        "border-color" => style.border_color = as_color(&v),
        // MOD4: a registered third-party property. Carried verbatim — the
        // framework does not interpret it, and deliberately cannot reach
        // `Style`'s built-in fields through this path.
        other if crate::registry::is_registered(other) => {
            style.custom.insert(other.to_string(), v);
        }
        _ => {}
    }
}

/// Parse `linear-gradient([<angle>deg,] <stop>…)` / `radial-gradient(<stop>…)`
/// where a stop is `<color> [<pct>]`. Stops without positions distribute
/// evenly; needs ≥ 2 colors.
fn as_gradient(name: &str, args: &[Value]) -> Option<StyleGradient> {
    let a = flat_args(args);
    let mut angle_deg = if name == "linear-gradient" {
        Some(180.0f32) // CSS default: to bottom
    } else {
        None
    };
    let mut stops: Vec<(Option<f32>, Color)> = Vec::new();
    for it in a {
        match it {
            Value::Number(n, Unit::Deg) if name == "linear-gradient" && stops.is_empty() => {
                angle_deg = Some(*n as f32);
            }
            Value::Number(n, Unit::Percent) => {
                if let Some(last) = stops.last_mut() {
                    last.0 = Some(*n as f32 / 100.0);
                }
            }
            other => {
                if let Some(c) = as_color(other) {
                    stops.push((None, c));
                }
            }
        }
    }
    if stops.len() < 2 {
        return None;
    }
    let n = stops.len();
    let stops = stops
        .into_iter()
        .enumerate()
        .map(|(i, (off, c))| (off.unwrap_or(i as f32 / (n - 1) as f32), c))
        .collect();
    Some(StyleGradient { angle_deg, stops })
}

/// Parse `shadow: <dx> <dy> [blur] [spread] <color>` (04 §3). Offsets and
/// radii are px numbers in order; the color ends the shadow (so a comma list
/// degrades to its first shadow). `inset` is unsupported — its presence
/// disables the declaration rather than painting an outer shadow wrongly.
fn as_shadow(v: &Value) -> Option<StyleShadow> {
    let items: Vec<&Value> = match v {
        Value::List(items) => items.iter().collect(),
        other => vec![other],
    };
    let mut nums: Vec<f32> = Vec::new();
    let mut color = None;
    for it in items {
        if matches!(it, Value::Keyword(k) if k == "inset") {
            return None;
        }
        if let Some(c) = as_color(it) {
            color = Some(c);
            break;
        }
        if let Some(px) = as_px(it) {
            if nums.len() < 4 {
                nums.push(px);
            }
        }
    }
    let color = color?;
    if nums.len() < 2 {
        return None;
    }
    Some(StyleShadow {
        dx: nums[0],
        dy: nums[1],
        blur: nums.get(2).copied().unwrap_or(0.0),
        spread: nums.get(3).copied().unwrap_or(0.0),
        color,
    })
}

/// Parse a per-side border: `<width> <color>` (either order, keywords like
/// `solid` ignored — matching the shorthand). Defaults: 1px, opaque black.
fn as_side_border(v: &Value) -> Option<StyleSideBorder> {
    let items: Vec<&Value> = match v {
        Value::List(items) => items.iter().collect(),
        other => vec![other],
    };
    let mut width = None;
    let mut color = None;
    for it in items {
        if let Some(px) = as_px(it) {
            width = Some(px);
        } else if let Some(c) = as_color(it) {
            color = Some(c);
        }
    }
    if width.is_none() && color.is_none() {
        return None;
    }
    Some(StyleSideBorder {
        width: width.unwrap_or(1.0),
        color: color.unwrap_or(Color::srgb8(0, 0, 0, 0xff)),
    })
}

/// Parse the `border: <width> <color>` shorthand (either order) into the typed
/// `border_width` / `border_color` fields. Per-side borders are not parsed yet.
fn apply_border(style: &mut Style, v: &Value) {
    let items: Vec<&Value> = match v {
        Value::List(items) => items.iter().collect(),
        other => vec![other],
    };
    for it in items {
        if let Some(px) = as_px(it) {
            style.border_width = Some(px);
        } else if let Some(c) = as_color(it) {
            style.border_color = Some(c);
        }
    }
}

/// Parse `backdrop-filter: blur(<px>) [saturate(<n>|<pct>)]` into the typed
/// glass fields. Filter functions beyond `blur`/`saturate` are ignored.
fn apply_backdrop(style: &mut Style, v: &Value) {
    let mut one = |f: &Value| {
        if let Value::Function(name, args) = f {
            let a = flat_args(args);
            match name.as_str() {
                "blur" => {
                    if let Some(px) = a.first().and_then(|x| as_px(x)) {
                        style.backdrop_blur = Some(px);
                    }
                }
                "saturate" => {
                    if let Some(s) = a.first().and_then(|x| as_saturate(x)) {
                        style.backdrop_saturate = Some(s);
                    }
                }
                "refraction" => {
                    if let Some(px) = a.first().and_then(|x| as_px(x)) {
                        style.backdrop_refraction = Some(px);
                    }
                }
                "specular" => {
                    if let Some(n) = a.first().and_then(|x| as_number(x)) {
                        style.backdrop_specular = Some(n as f32);
                    }
                }
                _ => {}
            }
        }
    };
    match v {
        Value::List(items) => {
            for it in items {
                one(it);
            }
        }
        other => one(other),
    }
}

/// A `saturate()` argument: a bare number (`1.8`) or a percentage (`180%`).
fn as_saturate(v: &Value) -> Option<f32> {
    match v {
        Value::Number(n, Unit::Percent) => Some(*n as f32 / 100.0),
        Value::Number(n, _) => Some(*n as f32),
        _ => None,
    }
}

/// Resolve `$token` references (one level) against `tokens`. Public so the
/// runtime can store *resolved* computed values for `ui.getStyles` (04 §7).
pub fn resolve_token(v: &Value, tokens: &Tokens) -> Value {
    match v {
        Value::Var(name) => tokens
            .get(name)
            .cloned()
            .unwrap_or(Value::Var(name.clone())),
        // Deep resolution (B.7): `$token`s nested in shorthands
        // (`border: 1px solid $border`) and function arguments
        // (`oklch(from $primary …)`) resolve too — still one level per
        // reference, matching the top-level rule.
        Value::List(items) => Value::List(items.iter().map(|i| resolve_token(i, tokens)).collect()),
        Value::Function(name, args) => Value::Function(
            name.clone(),
            args.iter().map(|a| resolve_token(a, tokens)).collect(),
        ),
        other => other.clone(),
    }
}

fn as_color(v: &Value) -> Option<Color> {
    match v {
        Value::Color(c) => Some(*c),
        Value::Function(name, args) if name == "oklch" => {
            let a = flat_args(args);
            // Relative form (04 §4, B.7): `oklch(from <color> L C H)` where
            // each channel is a number, the keyword `l`/`c`/`h` (the base's
            // channel), or `calc(…)` over those.
            if matches!(a.first(), Some(Value::Keyword(k)) if k == "from") {
                let base = as_color(a.get(1)?)?;
                let (bl, bc, bh) = base.to_oklch();
                let ch = |i: usize| channel_value(a.get(i).copied()?, bl, bc, bh);
                let mut out = Color::from_oklch(ch(2)? as f32, ch(3)? as f32, ch(4)? as f32);
                out.a = base.a;
                return Some(out);
            }
            let n = |i: usize| as_number(a.get(i).copied()?).map(|x| x as f32);
            Some(Color::from_oklch(n(0)?, n(1)?, n(2)?))
        }
        Value::Function(name, args) if name == "rgb" => {
            let a = flat_args(args);
            let n = |i: usize| as_number(a.get(i).copied()?).map(|x| x as u8);
            Some(Color::srgb8(n(0)?, n(1)?, n(2)?, 255))
        }
        _ => None,
    }
}

/// Parse `animation: <name> <dur> [easing] [delay] [count|infinite]
/// [alternate]` (B.5b). The first keyword is the `@keyframes` name; a bare
/// unitless number is the iteration count.
fn parse_animation(v: &Value) -> Option<AnimationSpec> {
    let items: Vec<&Value> = match v {
        Value::List(items) => items.iter().collect(),
        other => vec![other],
    };
    let mut spec: Option<AnimationSpec> = None;
    let mut durations_seen = 0u8;
    for it in items {
        match it {
            Value::Keyword(k) if spec.is_none() => {
                spec = Some(AnimationSpec {
                    name: k.clone(),
                    duration_ms: 0.0,
                    easing: crate::anim::Easing::Ease,
                    delay_ms: 0.0,
                    count: Some(1.0),
                    alternate: false,
                });
            }
            Value::Number(n, Unit::Ms) | Value::Number(n, Unit::S) => {
                let ms = if matches!(it, Value::Number(_, Unit::S)) {
                    *n * 1000.0
                } else {
                    *n
                };
                if let Some(sp) = &mut spec {
                    if durations_seen == 0 {
                        sp.duration_ms = ms as f32;
                    } else {
                        sp.delay_ms = ms as f32;
                    }
                    durations_seen += 1;
                }
            }
            Value::Number(n, Unit::None) => {
                if let Some(sp) = &mut spec {
                    sp.count = Some(*n as f32);
                }
            }
            Value::Keyword(k) => {
                if let Some(sp) = &mut spec {
                    match k.as_str() {
                        "infinite" => sp.count = None,
                        "alternate" => sp.alternate = true,
                        "linear" => sp.easing = crate::anim::Easing::Linear,
                        "ease" => sp.easing = crate::anim::Easing::Ease,
                        "ease-in" => sp.easing = crate::anim::Easing::EaseIn,
                        "ease-out" => sp.easing = crate::anim::Easing::EaseOut,
                        "ease-in-out" => sp.easing = crate::anim::Easing::EaseInOut,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    spec.filter(|sp| sp.duration_ms > 0.0)
}

/// Parse `transition: <prop|all> <dur> [<easing>] [<delay>][, …]` (B.5).
/// The comma-flattened atom list is re-grouped at each keyword that starts a
/// new declaration (a known property name or `all`).
fn parse_transitions(v: &Value) -> Vec<Transition> {
    let items: Vec<&Value> = match v {
        Value::List(items) => items.iter().collect(),
        other => vec![other],
    };
    let mut out = Vec::new();
    let mut cur: Option<Transition> = None;
    let mut durations_seen = 0u8;
    for it in items {
        match it {
            Value::Keyword(k)
                if k == "all" || crate::properties::KNOWN_PROPERTIES.contains(&k.as_str()) =>
            {
                if let Some(t) = cur.take() {
                    out.push(t);
                }
                durations_seen = 0;
                cur = Some(Transition {
                    property: k.clone(),
                    duration_ms: 0.0,
                    easing: crate::anim::Easing::Ease,
                    delay_ms: 0.0,
                });
            }
            Value::Number(n, Unit::Ms) => {
                if let Some(t) = &mut cur {
                    if durations_seen == 0 {
                        t.duration_ms = *n as f32;
                    } else {
                        t.delay_ms = *n as f32;
                    }
                    durations_seen += 1;
                }
            }
            Value::Number(n, Unit::S) => {
                if let Some(t) = &mut cur {
                    if durations_seen == 0 {
                        t.duration_ms = (*n * 1000.0) as f32;
                    } else {
                        t.delay_ms = (*n * 1000.0) as f32;
                    }
                    durations_seen += 1;
                }
            }
            Value::Keyword(k) => {
                if let Some(t) = &mut cur {
                    t.easing = match k.as_str() {
                        "linear" => crate::anim::Easing::Linear,
                        "ease" => crate::anim::Easing::Ease,
                        "ease-in" => crate::anim::Easing::EaseIn,
                        "ease-out" => crate::anim::Easing::EaseOut,
                        "ease-in-out" => crate::anim::Easing::EaseInOut,
                        _ => t.easing,
                    };
                }
            }
            _ => {}
        }
    }
    if let Some(t) = cur.take() {
        out.push(t);
    }
    out
}

/// One relative-color channel: a literal number, the base channel keyword
/// (`l`/`c`/`h`), or `calc(…)` over those.
fn channel_value(v: &Value, bl: f32, bc: f32, bh: f32) -> Option<f64> {
    match v {
        Value::Number(n, _) => Some(*n),
        Value::Keyword(k) => match k.as_str() {
            "l" => Some(bl as f64),
            "c" => Some(bc as f64),
            "h" => Some(bh as f64),
            _ => None,
        },
        Value::Function(name, args) if name == "calc" => eval_calc(&flat_args(args), bl, bc, bh),
        _ => None,
    }
}

/// Evaluate a `calc(…)` atom sequence left-to-right over `+`/`-`/`*` (no
/// precedence — matches the simple channel arithmetic 04 §4 shows; operators
/// need surrounding spaces, as in CSS).
fn eval_calc(atoms: &[&Value], bl: f32, bc: f32, bh: f32) -> Option<f64> {
    let mut acc = channel_value(atoms.first()?, bl, bc, bh)?;
    let mut i = 1;
    while i < atoms.len() {
        let Value::Keyword(op) = atoms[i] else {
            return None;
        };
        let rhs = channel_value(atoms.get(i + 1).copied()?, bl, bc, bh)?;
        match op.as_str() {
            "+" => acc += rhs,
            "-" => acc -= rhs,
            "*" => acc *= rhs,
            _ => return None,
        }
        i += 2;
    }
    Some(acc)
}

/// Flatten a single space/comma list argument into its items (CSS color
/// functions write `oklch(L C H)` / `rgb(r, g, b)`, which the value parser
/// collects into one list).
fn flat_args(args: &[Value]) -> Vec<&Value> {
    if let [Value::List(items)] = args {
        items.iter().collect()
    } else {
        args.iter().collect()
    }
}

fn as_dim(v: &Value) -> Option<Dim> {
    match v {
        Value::Number(n, Unit::Px) => Some(Dim::px(*n as f32)),
        Value::Number(n, Unit::Percent) => Some(Dim::pct(*n as f32 / 100.0)),
        Value::Number(n, Unit::None) => Some(Dim::px(*n as f32)),
        Value::Keyword(k) if k == "auto" => Some(Dim::Auto),
        _ => None,
    }
}

fn as_px(v: &Value) -> Option<f32> {
    match v {
        Value::Number(n, Unit::Px | Unit::None) => Some(*n as f32),
        _ => None,
    }
}

fn as_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n, _) => Some(*n),
        _ => None,
    }
}

fn as_display(v: &Value) -> Option<Display> {
    match v {
        Value::Keyword(k) => match k.as_str() {
            "flex" => Some(Display::Flex),
            "grid" => Some(Display::Grid),
            "none" => Some(Display::None),
            _ => None,
        },
        _ => None,
    }
}

/// PROP1: CSS alignment keywords → [`Align`].
///
/// `flex-start`/`flex-end` are accepted alongside `start`/`end` because CSS
/// authors write both and the difference is not meaningful here.
fn as_align(v: &Value) -> Option<Align> {
    match v {
        Value::Keyword(k) => match k.as_str() {
            "start" | "flex-start" => Some(Align::Start),
            "end" | "flex-end" => Some(Align::End),
            "center" => Some(Align::Center),
            "stretch" => Some(Align::Stretch),
            "baseline" => Some(Align::Baseline),
            "space-between" => Some(Align::SpaceBetween),
            "space-around" => Some(Align::SpaceAround),
            _ => None,
        },
        _ => None,
    }
}

/// `z-index` (PROP1). An integer, or `auto` (which is 0 — the default layer).
///
/// **Non-negative only.** `Tree`'s per-node `z` is a `u32` with `0` as the
/// default, so there is no room below an unstyled sibling: a negative value
/// cannot be represented without widening that field, which would touch the
/// hit-test ordering and the overlay constant too. Rejected (`W0109`) rather
/// than silently clamped to 0, since clamping would make `z-index: -1` look
/// like it worked.
fn as_z_index(v: &Value) -> Option<i32> {
    match v {
        Value::Keyword(k) if k == "auto" => Some(0),
        Value::Number(n, Unit::None) if *n >= 0.0 => Some(*n as i32),
        _ => None,
    }
}

/// `filter` (PROP1) — **`blur()` only**, plus `none`.
///
/// CSS's colour-map filters (`brightness`, `contrast`, `saturate`, …) are cheap
/// to add on top of this plumbing but are per-pixel maps the layer path does not
/// yet apply; accepting them now would render nothing and be the silent no-op
/// this series removed 26 instances of. They report `W0109` until implemented.
fn as_filter_blur(v: &Value) -> Option<f32> {
    match v {
        Value::Keyword(k) if k == "none" => Some(0.0),
        Value::Function(name, args) if name == "blur" => match args.first() {
            // px or unitless; a percentage is meaningless for a blur radius.
            Some(Value::Number(n, Unit::Px | Unit::None)) if *n >= 0.0 => Some(*n as f32),
            _ => None,
        },
        _ => None,
    }
}

/// `transform` (PROP1): a CSS function list composed into one affine, applied
/// left-to-right as CSS specifies.
///
/// Supported: `translate(x[, y])`, `translateX/Y`, `scale(s[, sy])`,
/// `scaleX/Y`, `rotate(deg)`, `skewX/Y(deg)`, `none`. Lengths are px;
/// percentages are NOT accepted for translate, because a percentage there is
/// relative to the node's own box, which the style engine cannot see — honouring
/// it needs the resolved bounds, so it belongs to a later pass rather than a
/// silent misinterpretation here.
fn as_transform(v: &Value) -> Option<kurbo::Affine> {
    let items: Vec<&Value> = match v {
        Value::List(xs) => xs.iter().collect(),
        single => vec![single],
    };
    if let [Value::Keyword(k)] = items.as_slice() {
        if k.as_str() == "none" {
            return Some(kurbo::Affine::IDENTITY);
        }
    }
    let mut out = kurbo::Affine::IDENTITY;
    let mut any = false;
    for item in items {
        let Value::Function(name, args) = item else {
            return None;
        };
        // NOT `as_number`: it ignores the unit, which would silently read
        // `translate(50%)` as 50 PX. A percentage there is relative to the
        // node's own box, which this layer cannot see, so it must be rejected
        // rather than misread. Angles are accepted for the rotate/skew
        // functions; the unit is checked by the caller's choice of function.
        let num = |i: usize| -> Option<f64> {
            match args.get(i) {
                Some(Value::Number(n, Unit::None | Unit::Px | Unit::Deg)) => Some(*n),
                _ => None,
            }
        };
        let t = match name.as_str() {
            "translate" => kurbo::Affine::translate((num(0)?, num(1).unwrap_or(0.0))),
            "translateX" => kurbo::Affine::translate((num(0)?, 0.0)),
            "translateY" => kurbo::Affine::translate((0.0, num(0)?)),
            "scale" => {
                let sx = num(0)?;
                kurbo::Affine::scale_non_uniform(sx, num(1).unwrap_or(sx))
            }
            "scaleX" => kurbo::Affine::scale_non_uniform(num(0)?, 1.0),
            "scaleY" => kurbo::Affine::scale_non_uniform(1.0, num(0)?),
            "rotate" => kurbo::Affine::rotate(num(0)?.to_radians()),
            "skewX" => kurbo::Affine::skew(num(0)?.to_radians().tan(), 0.0),
            "skewY" => kurbo::Affine::skew(0.0, num(0)?.to_radians().tan()),
            _ => return None,
        };
        out *= t;
        any = true;
    }
    any.then_some(out)
}

/// `transform-origin` (PROP1) as a fraction of the box. Keywords and
/// percentages only — a px origin would need the resolved bounds, same reason
/// as translate percentages.
fn as_transform_origin(v: &Value) -> Option<(f64, f64)> {
    fn axis(v: &Value) -> Option<f64> {
        match v {
            Value::Keyword(k) => match k.as_str() {
                "left" | "top" => Some(0.0),
                "center" => Some(0.5),
                "right" | "bottom" => Some(1.0),
                _ => None,
            },
            Value::Number(n, Unit::Percent) => Some(n / 100.0),
            _ => None,
        }
    }
    match v {
        Value::List(xs) if xs.len() == 2 => Some((axis(&xs[0])?, axis(&xs[1])?)),
        single => axis(single).map(|a| (a, a)),
    }
}

/// `font-features` **and `font-variation`** (PROP1) — both are quoted-tag lists
/// in the same CSS shape (`"wght" 700`), handed to the shaper verbatim in CSS
/// `font-feature-settings` syntax, so any tag the face carries works and any it
/// does not is ignored — matching CSS, and avoiding a hardcoded tag allow-list
/// that would go stale against whatever font an app registers.
///
/// `normal` clears it. A bare keyword other than `normal` is rejected: feature
/// settings are quoted tags, and an unquoted word is a typo rather than a
/// setting.
fn as_feature_settings(v: &Value) -> Option<String> {
    match v {
        // The lexer strips the quotes, but the shaper parses CSS syntax and
        // needs them back — an unquoted tag is not a valid setting.
        Value::Str(s) => Some(format!("\"{s}\"")),
        Value::Keyword(k) if k == "normal" => Some(String::new()),
        // `"tnum" 1` lexes as a list; rebuild the source text for the shaper.
        Value::List(items) => {
            let parts: Vec<String> = items
                .iter()
                .map(|i| match i {
                    Value::Str(s) => format!("\"{s}\""),
                    Value::Number(n, _) => format!("{n}"),
                    Value::Keyword(k) => k.clone(),
                    _ => String::new(),
                })
                .collect();
            let joined = parts.join(" ");
            (!joined.trim().is_empty()).then_some(joined)
        }
        _ => None,
    }
}

/// `text-overflow` (PROP1). `clip` and `ellipsis` only — CSS also allows an
/// arbitrary replacement string, which would need its own width measurement per
/// node and buys little over the ellipsis.
fn as_text_overflow(v: &Value) -> Option<bool> {
    match v {
        Value::Keyword(k) => match k.as_str() {
            "clip" => Some(false),
            "ellipsis" => Some(true),
            _ => None,
        },
        _ => None,
    }
}

/// `text-wrap` (PROP1). `wrap` and `nowrap` only — CSS's `balance` and
/// `pretty` ask the shaper to optimise line breaks across the whole paragraph,
/// which parley does not expose, and accepting them as aliases for `wrap` would
/// silently do nothing.
fn as_text_wrap(v: &Value) -> Option<bool> {
    match v {
        Value::Keyword(k) => match k.as_str() {
            "wrap" => Some(true),
            "nowrap" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// `text-decoration` (PROP1). Only the two lines the paint layer can draw as a
/// rect. CSS's `overline`, and the `text-decoration-style`/`-color` longhands,
/// are not accepted — a dotted or coloured underline needs more than a filled
/// rect, and claiming them would be the silent no-op this series removes.
fn as_text_decoration(v: &Value) -> Option<lumen_core::TextDecoration> {
    use lumen_core::TextDecoration as D;
    match v {
        Value::Keyword(k) => match k.as_str() {
            "none" => Some(D::None),
            "underline" => Some(D::Underline),
            "line-through" => Some(D::LineThrough),
            _ => None,
        },
        _ => None,
    }
}

/// `cursor` (PROP1). A deliberately small set — the shapes every platform
/// actually has. CSS defines ~35; most are aliases or X11-era curiosities, and
/// accepting a name the shell cannot render would be the silent-no-op this
/// whole series exists to remove. Unlisted names report `W0109`.
fn as_cursor(v: &Value) -> Option<lumen_core::CursorShape> {
    use lumen_core::CursorShape as C;
    match v {
        Value::Keyword(k) => match k.as_str() {
            "default" | "auto" => Some(C::Default),
            "pointer" => Some(C::Pointer),
            "text" => Some(C::Text),
            "wait" | "progress" => Some(C::Wait),
            "crosshair" => Some(C::Crosshair),
            "move" | "grab" | "grabbing" => Some(C::Move),
            "col-resize" | "ew-resize" => Some(C::ColResize),
            "row-resize" | "ns-resize" => Some(C::RowResize),
            "not-allowed" => Some(C::NotAllowed),
            "none" => Some(C::None),
            _ => None,
        },
        _ => None,
    }
}

/// `font-style` (PROP1). `oblique` is accepted as a synonym for `italic`:
/// the bundled face has neither, so both resolve to the same synthetic skew and
/// pretending to distinguish them would be a lie about what is rendered.
fn as_font_style(v: &Value) -> Option<bool> {
    match v {
        Value::Keyword(k) => match k.as_str() {
            "italic" | "oblique" => Some(true),
            "normal" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// `text-align` (PROP1). `justify` is NOT accepted: the shaper has no
/// justification pass, so accepting it would claim support Lumen does not have.
/// Rejecting leaves the text at `start` — visually the same, but the property
/// reads as unset rather than as honoured. (No diagnostic fires for a rejected
/// value; see `as_overflow` for the open value-level hole.)
fn as_text_align(v: &Value) -> Option<lumen_text::TextAlign> {
    match v {
        Value::Keyword(k) => match k.as_str() {
            "start" | "left" => Some(lumen_text::TextAlign::Start),
            "center" => Some(lumen_text::TextAlign::Center),
            "end" | "right" => Some(lumen_text::TextAlign::End),
            _ => None,
        },
        _ => None,
    }
}

/// `overflow` (PROP1) mapped onto [`StyleClip`].
///
/// `hidden`/`clip` -> `Rounded`, which follows the node's `border-radius` — the
/// same thing CSS does (a rounded box clips to its curve). `visible` -> `None`.
///
/// **`scroll` and `auto` are deliberately rejected.** Scrolling in Lumen is a
/// widget (`Scrollable`), not a paint property: there is no scroll offset,
/// scrollbar or wheel routing attached to a style declaration. Accepting them
/// as a silent alias for `hidden` would produce a box that clips its content
/// with no way to reach the rest — the failure looks like lost content rather
/// than an unsupported value. Rejected, so the property simply does not apply.
///
/// NOTE: no diagnostic fires. `W0107` reports an unimplemented *property*; a
/// rejected *value* on an implemented property is still silent (the value-level
/// hole, SD5.x, open — verified 2026-08-08). `ui.explain {kind: "style"}` is the
/// way to see it today.
fn as_overflow(v: &Value) -> Option<StyleClip> {
    match v {
        Value::Keyword(k) => match k.as_str() {
            "hidden" | "clip" => Some(StyleClip::Rounded),
            "visible" => Some(StyleClip::None),
            _ => None,
        },
        _ => None,
    }
}

/// `grid-template-columns` / `-rows` (PROP1): a space-separated track list, e.g.
/// `1fr 2fr 100px auto`. `repeat()` is not supported — write the tracks out.
///
/// The whole declaration is rejected if ANY track is unparseable, rather than
/// silently dropping that track: a grid missing one column would lay out
/// plausibly but wrongly, which is far harder to notice than the property not
/// applying at all. (No diagnostic fires for a rejected *value* — see
/// `as_overflow` for the open value-level hole.)
fn as_grid_tracks(v: &Value) -> Option<Vec<GridTrack>> {
    let items: Vec<&Value> = match v {
        Value::List(xs) => xs.iter().collect(),
        single => vec![single],
    };
    let tracks: Option<Vec<GridTrack>> = items.into_iter().map(as_grid_track).collect();
    tracks.filter(|t| !t.is_empty())
}

fn as_grid_track(v: &Value) -> Option<GridTrack> {
    match v {
        Value::Keyword(k) if k == "auto" => Some(GridTrack::Auto),
        Value::Number(n, Unit::Fr) => Some(GridTrack::Fr(*n as f32)),
        Value::Number(n, Unit::Px) => Some(GridTrack::Px(*n as f32)),
        Value::Number(n, Unit::Percent) => Some(GridTrack::Percent(*n as f32)),
        _ => None,
    }
}

/// `grid-column` / `grid-row` (PROP1): `auto`, `span <n>`, or a line number.
///
/// CSS's `<start> / <end>` form is NOT accepted — the lexer treats `/` only as
/// a comment opener, so supporting it needs lexer work rather than a parse
/// helper (the same limitation `aspect-ratio` has). A single value sets the
/// START edge and leaves the end `Auto`, which is what CSS does too.
fn as_grid_line_pair(v: &Value) -> Option<(GridLine, GridLine)> {
    Some((as_grid_line(v)?, GridLine::Auto))
}

fn as_grid_line(v: &Value) -> Option<GridLine> {
    match v {
        Value::Keyword(k) if k == "auto" => Some(GridLine::Auto),
        Value::Number(n, Unit::None) => Some(GridLine::Line(*n as i16)),
        Value::List(xs) => match xs.as_slice() {
            [Value::Keyword(k), Value::Number(n, Unit::None)] if k == "span" => {
                let n = *n as i64;
                (n > 0).then_some(GridLine::Span(n as u16))
            }
            _ => None,
        },
        _ => None,
    }
}

/// `font-family` (PROP1): a quoted string or a bare keyword. A comma-separated
/// CSS font stack is NOT resolved — shaping uses exactly one registered family
/// (ADR-005 forbids system-font enumeration, so there is no list to fall
/// through). The first entry wins and the rest are ignored, which is closer to
/// CSS intent than rejecting the declaration outright.
fn as_font_family(v: &Value) -> Option<String> {
    match v {
        Value::Str(s) | Value::Keyword(s) => Some(s.clone()),
        Value::List(items) => items.iter().find_map(as_font_family),
        _ => None,
    }
}

/// `position: relative | absolute` (PROP1).
fn as_position(v: &Value) -> Option<Position> {
    match v {
        Value::Keyword(s) => match s.as_str() {
            "relative" => Some(Position::Relative),
            "absolute" => Some(Position::Absolute),
            _ => None,
        },
        _ => None,
    }
}

/// `aspect-ratio` (PROP1) as a bare number: width ÷ height, so `2` is twice as
/// wide as tall.
///
/// CSS's `16 / 9` form is **not** accepted: the lexer only treats `/` as the
/// start of a comment, so supporting it needs lexer work rather than a parse
/// helper. `aspect-ratio: 1.778` is the spelling that works today.
///
/// Zero, negative and non-finite values are rejected rather than passed to
/// taffy, where they collapse the node — a stylesheet typo should leave the
/// property unset rather than vanish the element. (No diagnostic fires for a
/// rejected value; see `as_overflow`.)
fn as_aspect_ratio(v: &Value) -> Option<f32> {
    let ratio = as_number(v)? as f32;
    (ratio.is_finite() && ratio > 0.0).then_some(ratio)
}

fn as_flex_wrap(v: &Value) -> Option<FlexWrap> {
    match v {
        Value::Keyword(k) => match k.as_str() {
            "nowrap" => Some(FlexWrap::NoWrap),
            "wrap" => Some(FlexWrap::Wrap),
            "wrap-reverse" => Some(FlexWrap::WrapReverse),
            _ => None,
        },
        _ => None,
    }
}

fn as_flex_direction(v: &Value) -> Option<FlexDirection> {
    match v {
        Value::Keyword(k) => match k.as_str() {
            "row" => Some(FlexDirection::Row),
            "column" => Some(FlexDirection::Column),
            "row-reverse" => Some(FlexDirection::RowReverse),
            "column-reverse" => Some(FlexDirection::ColumnReverse),
            _ => None,
        },
        _ => None,
    }
}

/// Serialize a computed value to the `ui.getStyles` canonical form (04 §7):
/// `{ "value": <canonical>, "source": "theme|stylesheet|inline|default" }`.
/// Canonical forms: lengths `{px}`, colors `#rrggbbaa`, enums as strings.
/// Introspection surface — present only in a `snapshot` build.
#[cfg(feature = "snapshot")]
pub fn computed_json(value: &Value, origin: Origin) -> Json {
    json!({ "value": canonical(value), "source": source_str(origin) })
}

/// [`computed_json`] with the winning declaration's source span (B.7b,
/// 04 §7) — `{line, col}` into the stylesheet the app loaded.
#[cfg(feature = "snapshot")]
pub fn computed_json_spanned(
    value: &Value,
    origin: Origin,
    span: Option<crate::ast::Span>,
) -> Json {
    let mut j = computed_json(value, origin);
    if let Some(sp) = span {
        j["span"] = json!({ "line": sp.line, "col": sp.col });
    }
    j
}

#[cfg(feature = "snapshot")]
fn source_str(origin: Origin) -> &'static str {
    match origin {
        Origin::Default => "default",
        Origin::Theme => "theme",
        Origin::App => "stylesheet",
        Origin::Inline => "inline",
    }
}

/// The canonical JSON form of a value (04 §7).
#[cfg(feature = "snapshot")]
pub fn canonical(value: &Value) -> Json {
    match value {
        Value::Number(n, Unit::Px | Unit::None) => json!({ "px": n }),
        Value::Number(n, Unit::Percent) => json!({ "percent": n }),
        Value::Number(n, Unit::Ms) => json!({ "ms": n }),
        Value::Number(n, Unit::S) => json!({ "ms": n * 1000.0 }),
        Value::Number(n, Unit::Deg) => json!({ "deg": n }),
        Value::Number(n, Unit::Fr) => json!({ "fr": n }),
        Value::Color(c) => json!(c.to_hex()),
        Value::Keyword(k) => json!(k),
        Value::Str(s) => json!(s),
        Value::Var(v) => json!(format!("${v}")),
        Value::Function(name, args) => {
            json!({ "fn": name, "args": args.iter().map(canonical).collect::<Vec<_>>() })
        }
        Value::List(items) => json!(items.iter().map(canonical).collect::<Vec<_>>()),
    }
}
