//! The `.lss` abstract syntax tree (04 §1).

use lumen_core::Color;

/// A source position (1-based line/col), used for diagnostics spans.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Span {
    /// 1-based line.
    pub line: u32,
    /// 1-based column.
    pub col: u32,
}

/// A parsed stylesheet.
#[derive(Clone, Debug, Default)]
pub struct Stylesheet {
    /// Top-level items in source order.
    pub items: Vec<Item>,
}

/// A top-level item.
#[derive(Clone, Debug)]
pub enum Item {
    /// A style rule.
    Rule(Rule),
    /// `@tokens { … }`.
    Tokens(Vec<Binding>),
    /// `@theme light|dark|high-contrast { … }`.
    Theme(ThemeKind, Vec<Binding>),
    /// `@keyframes name { … }`.
    Keyframes(Keyframes),
    /// `@media (…) { … }`.
    Media(Vec<MediaQuery>, Vec<Rule>),
}

/// A `name: value;` binding (in `@tokens`/`@theme`).
#[derive(Clone, Debug)]
pub struct Binding {
    /// The token name.
    pub name: String,
    /// The value.
    pub value: Value,
    /// Source span of the binding.
    pub span: Span,
}

/// A theme variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeKind {
    /// Light theme.
    Light,
    /// Dark theme.
    Dark,
    /// High-contrast theme.
    HighContrast,
}

/// `@keyframes name { 0% { … } 100% { … } }`.
#[derive(Clone, Debug)]
pub struct Keyframes {
    /// Animation name.
    pub name: String,
    /// `(percent, declarations)` stops.
    pub stops: Vec<(f32, Vec<Declaration>)>,
}

/// A style rule: `selectors { declarations; nested }`.
#[derive(Clone, Debug)]
pub struct Rule {
    /// One or more comma-separated selectors.
    pub selectors: Vec<Selector>,
    /// Declarations.
    pub declarations: Vec<Declaration>,
    /// Nested `& …` rules.
    pub nested: Vec<NestedRule>,
}

/// A nested `& part+ { … }` (or `& > part+ { … }`) rule.
#[derive(Clone, Debug)]
pub struct NestedRule {
    /// Parts appended to `&` (the parent) — or, when `child`, the parts of a
    /// new child compound (`& > .thumb`).
    pub parts: Vec<Part>,
    /// `& > part+`: the parts form a child compound instead of extending the
    /// parent's (B.1).
    pub child: bool,
    /// Declarations.
    pub declarations: Vec<Declaration>,
}

/// `property: value (!important)?;`.
#[derive(Clone, Debug)]
pub struct Declaration {
    /// Property name.
    pub property: String,
    /// Value.
    pub value: Value,
    /// Whether `!important` was present.
    pub important: bool,
    /// Source span (of the property name).
    pub span: Span,
}

/// A selector: compound (combinator compound)*.
#[derive(Clone, Debug)]
pub struct Selector {
    /// The first compound and subsequent `(combinator, compound)` pairs.
    pub first: Compound,
    /// Trailing combinator+compound pairs.
    pub rest: Vec<(Combinator, Compound)>,
    /// Source span (of the selector start).
    pub span: Span,
}

/// A compound selector: one or more simple parts.
#[derive(Clone, Debug, Default)]
pub struct Compound {
    /// Simple parts (id/class/type/state/any).
    pub parts: Vec<Part>,
}

/// A simple selector part (subset of 03 §2 used in `.lss`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// `#id`.
    Id(String),
    /// `.class`.
    Class(String),
    /// bare type/role word.
    Type(String),
    /// `:state`.
    State(String),
    /// `*`.
    Any,
}

/// A selector combinator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combinator {
    /// Descendant (whitespace).
    Descendant,
    /// Direct child (`>`).
    Child,
}

/// A parsed value (one atom, or a space/comma list of atoms).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A length with a unit (px/%/ms/s/deg) or unitless number.
    Number(f64, Unit),
    /// A color literal.
    Color(Color),
    /// A bare keyword (e.g. `flex`, `solid`, `ease`).
    Keyword(String),
    /// A double-quoted string.
    Str(String),
    /// A `$token` reference.
    Var(String),
    /// `name(args…)`.
    Function(String, Vec<Value>),
    /// A space- or comma-separated list.
    List(Vec<Value>),
}

/// A numeric unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// Unitless.
    None,
    /// Logical pixels.
    Px,
    /// Percent.
    Percent,
    /// Milliseconds.
    Ms,
    /// Seconds.
    S,
    /// Degrees.
    Deg,
    /// Fractional grid-track unit (`1fr`, 04 §3).
    Fr,
}

/// A media query feature comparison.
#[derive(Clone, Debug)]
pub struct MediaQuery {
    /// Feature name (width/height/platform/pointer/scale).
    pub feature: String,
    /// Comparison operator.
    pub op: MediaOp,
    /// Comparison value.
    pub value: Value,
    /// `@media container(…)` — test the nearest `.container()` ancestor's
    /// size instead of the window (B.2b, 04 §6).
    pub container: bool,
}

/// A media-query comparison operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaOp {
    /// `:` (equals).
    Eq,
    /// `<`.
    Lt,
    /// `>`.
    Gt,
    /// `<=`.
    Le,
    /// `>=`.
    Ge,
}

/// CSS-style specificity `(id, class+state, type)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Specificity {
    /// Number of id parts.
    pub id: u32,
    /// Number of class + state parts.
    pub class: u32,
    /// Number of type parts.
    pub ty: u32,
}

impl Selector {
    /// Compute the selector's specificity (04 §2): `(id, class+state, type)`.
    pub fn specificity(&self) -> Specificity {
        let mut s = Specificity::default();
        let mut tally = |c: &Compound| {
            for p in &c.parts {
                match p {
                    Part::Id(_) => s.id += 1,
                    Part::Class(_) | Part::State(_) => s.class += 1,
                    Part::Type(_) => s.ty += 1,
                    Part::Any => {}
                }
            }
        };
        tally(&self.first);
        for (_, c) in &self.rest {
            tally(c);
        }
        s
    }
}
