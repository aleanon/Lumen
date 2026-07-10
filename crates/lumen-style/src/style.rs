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
use lumen_layout::{Dim, Display, Edges, FlexDirection};
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
    /// `visibility` (B.3): `Some(false)` = hidden — the subtree keeps its
    /// layout space but is removed from paint, hit-testing, and semantics.
    pub visibility: Option<bool>,
    /// `shadow` (B.3): single drop shadow. `inset` and comma lists are not
    /// supported yet (an `inset` keyword disables the declaration).
    pub shadow: Option<StyleShadow>,
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

/// The `.lss` properties `apply` actually consumes — the runtime's applied
/// set, in `apply` arm order. The parity test asserts (a) each entry really
/// changes a `Style`, (b) no other known property does, and (c) the typed
/// mirror covers exactly this set — so this const, `apply`, and the setters
/// cannot drift apart silently (04 §8).
pub const APPLIED_PROPERTIES: &[&str] = &[
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
    "background",
    "color",
    "border-radius",
    "opacity",
    "font-size",
    "font-weight",
    "line-height",
    "backdrop-filter",
    "shadow",
    "visibility",
    "border",
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
        "visibility" => {
            style.visibility = match &v {
                Value::Keyword(k) if k == "visible" => Some(true),
                Value::Keyword(k) if k == "hidden" => Some(false),
                _ => None,
            }
        }
        "border" => apply_border(style, &v),
        "border-width" => style.border_width = as_px(&v),
        "border-color" => style.border_color = as_color(&v),
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
