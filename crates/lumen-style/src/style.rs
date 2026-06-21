//! The typed `Style` — the 1:1 Rust mirror of `.lss` properties (04 §8), the
//! `.lss`→typed application path, and computed-value serialization (04 §7).
//!
//! `Style` setters and `.lss` declarations must agree; the `style_parity!`
//! test asserts that. M0/M1 covers the common property subset used by widgets
//! and the gallery; the remaining v1 properties slot in the same way.

use crate::ast::{Unit, Value};
use crate::Origin;
use lumen_core::Color;
use lumen_layout::{Dim, Display, Edges, FlexDirection};
use serde_json::{json, Value as Json};
use std::collections::HashMap;

/// A resolved token table (`@tokens` + the active `@theme`), name → value.
pub type Tokens = HashMap<String, Value>;

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
    /// `background` color.
    pub background: Option<Color>,
    /// `color` (text).
    pub color: Option<Color>,
    /// `border-radius` (uniform).
    pub border_radius: Option<f32>,
    /// `opacity`.
    pub opacity: Option<f32>,
    /// `font-size`.
    pub font_size: Option<f32>,
    /// `font-weight`.
    pub font_weight: Option<u16>,
    /// `backdrop-filter: blur(...)` radius in px (glass).
    pub backdrop_blur: Option<f32>,
    /// `backdrop-filter: saturate(...)` multiplier (`1.0` = none).
    pub backdrop_saturate: Option<f32>,
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
}

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
        "background" => style.background = as_color(&v),
        "color" => style.color = as_color(&v),
        "border-radius" => style.border_radius = as_px(&v),
        "opacity" => style.opacity = as_number(&v).map(|n| n as f32),
        "font-size" => style.font_size = as_px(&v),
        "font-weight" => style.font_weight = as_number(&v).map(|n| n as u16),
        "backdrop-filter" => apply_backdrop(style, &v),
        _ => {}
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
        other => other.clone(),
    }
}

fn as_color(v: &Value) -> Option<Color> {
    match v {
        Value::Color(c) => Some(*c),
        Value::Function(name, args) if name == "oklch" => {
            let a = flat_args(args);
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
pub fn computed_json(value: &Value, origin: Origin) -> Json {
    json!({ "value": canonical(value), "source": source_str(origin) })
}

fn source_str(origin: Origin) -> &'static str {
    match origin {
        Origin::Default => "default",
        Origin::Theme => "theme",
        Origin::App => "stylesheet",
        Origin::Inline => "inline",
    }
}

/// The canonical JSON form of a value (04 §7).
pub fn canonical(value: &Value) -> Json {
    match value {
        Value::Number(n, Unit::Px | Unit::None) => json!({ "px": n }),
        Value::Number(n, Unit::Percent) => json!({ "percent": n }),
        Value::Number(n, Unit::Ms) => json!({ "ms": n }),
        Value::Number(n, Unit::S) => json!({ "ms": n * 1000.0 }),
        Value::Number(n, Unit::Deg) => json!({ "deg": n }),
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
