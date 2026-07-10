//! `lumen-style` — the `.lss` styling language: lexer, parser, cascade, and
//! (later) tokens/themes/animation (04).
//!
//! The grammar is CSS-like by design (ADR-016) for AI-author familiarity.
//! Parsing is total and produces structured diagnostics (E0101–E0104 with
//! spans); a stylesheet with any error is rejected atomically so hot reload can
//! keep the previous one live (04 §9).
#![warn(missing_docs)]

pub mod anim;
pub mod ast;
pub mod lexer;
pub mod motion;
pub mod parser;
pub mod properties;
pub mod style;

pub use anim::{AnimValue, Easing, Scheduler};

pub use ast::{
    Combinator, Compound, Declaration, Item, Part, Rule, Selector, Specificity, Stylesheet,
    ThemeKind, Unit, Value,
};
pub use parser::{has_errors, parse};
pub use properties::KNOWN_PROPERTIES;
pub use style::{apply, resolve_token, Style, Tokens};
#[cfg(feature = "snapshot")]
pub use style::{canonical, computed_json, computed_json_spanned};

use std::collections::HashMap;

/// Cascade origin, low → high priority (04 §2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    /// Framework defaults.
    Default,
    /// `@theme` blocks.
    Theme,
    /// App stylesheets (in file order).
    App,
    /// Inline `.style(...)` from Rust.
    Inline,
}

/// A stylesheet tagged with its cascade origin.
pub struct StyleSource {
    /// The origin tier.
    pub origin: Origin,
    /// The parsed stylesheet.
    pub sheet: Stylesheet,
}

/// A node's identity for matching (the parts a selector can test).
#[derive(Clone, Debug, Default)]
pub struct NodeDesc {
    /// `#id`.
    pub id: Option<String>,
    /// `.class`es.
    pub classes: Vec<String>,
    /// `:state`s.
    pub states: Vec<String>,
    /// bare type/role.
    pub ty: String,
}

impl NodeDesc {
    /// Whether `selector` matches this node with **no** ancestors — i.e.
    /// single-compound selectors only. Prefer
    /// [`matches_chain`](Self::matches_chain); kept for the cascade tests.
    pub fn matches(&self, selector: &Selector) -> bool {
        self.matches_chain(selector, &[])
    }

    /// Whether `selector` matches this node given its `ancestors`
    /// (root-first, nearest parent last) — real descendant/`>` combinator
    /// matching, right-to-left (B.1). Before B.1 only the rightmost compound
    /// was checked, so `dialog button` matched *any* button.
    pub fn matches_chain(&self, selector: &Selector, ancestors: &[NodeDesc]) -> bool {
        let target = selector
            .rest
            .last()
            .map(|(_, c)| c)
            .unwrap_or(&selector.first);
        if !compound_matches(self, target) {
            return false;
        }
        // Remaining compounds, right-to-left: rest[i].0 joins compound i to
        // i+1, so walking down pairs each earlier compound with the
        // combinator on its right.
        let mut chain: Vec<(&Compound, Combinator)> = Vec::new();
        {
            let mut prev = &selector.first;
            for (comb, comp) in &selector.rest {
                chain.push((prev, *comb));
                prev = comp;
            }
        }
        let mut idx = ancestors.len(); // position just above `self`
        for (comp, comb) in chain.into_iter().rev() {
            match comb {
                Combinator::Child => {
                    if idx == 0 {
                        return false;
                    }
                    idx -= 1;
                    if !compound_matches(&ancestors[idx], comp) {
                        return false;
                    }
                }
                Combinator::Descendant => loop {
                    if idx == 0 {
                        return false;
                    }
                    idx -= 1;
                    if compound_matches(&ancestors[idx], comp) {
                        break;
                    }
                },
            }
        }
        true
    }

    fn matches_part(&self, p: &Part) -> bool {
        match p {
            Part::Any => true,
            Part::Id(s) => self.id.as_deref() == Some(s.as_str()),
            Part::Class(s) => self.classes.iter().any(|c| c == s),
            Part::State(s) => self.states.iter().any(|st| st == s),
            Part::Type(s) => &self.ty == s,
        }
    }
}

fn compound_matches(node: &NodeDesc, compound: &Compound) -> bool {
    compound.parts.iter().all(|p| node.matches_part(p))
}

/// The computed value of a property plus the cascade key that won it.
#[derive(Clone, Debug)]
pub struct Computed {
    /// The winning value.
    pub value: Value,
    /// Whether it was `!important`.
    pub important: bool,
    /// The origin it came from.
    pub origin: Origin,
    /// Source location of the winning declaration (B.7b, 04 §7) — `None`
    /// for origins without source text (e.g. future inline styles).
    pub span: Option<crate::ast::Span>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CascadeKey {
    important: bool,
    origin: Origin,
    specificity: Specificity,
    source: usize,
}

/// Resolve the cascade for `node` over `sources` (04 §2): `!important` beats
/// everything, then origin, then specificity `(id, class+state, type)`, then
/// source order. Returns the winning value per property. Ancestor-free and
/// evaluated against the default [`MediaContext`] — see
/// [`resolve_with_ancestors`] for the runtime's full form (B.1/B.2).
pub fn resolve(sources: &[StyleSource], node: &NodeDesc) -> HashMap<String, Computed> {
    resolve_with_ancestors(sources, node, &[], &MediaContext::default())
}

/// [`resolve`], with `node`'s ancestor chain (root-first) so descendant/`>`
/// selectors match correctly (B.1), and the live [`MediaContext`] so
/// `@media` blocks gate on the real window (B.2 — previously their rules
/// applied unconditionally). The runtime styler threads both.
pub fn resolve_with_ancestors(
    sources: &[StyleSource],
    node: &NodeDesc,
    ancestors: &[NodeDesc],
    ctx: &MediaContext,
) -> HashMap<String, Computed> {
    let mut winners: HashMap<String, (CascadeKey, Computed)> = HashMap::new();
    let mut source = 0usize;

    for src in sources {
        for rule in rules_in_ctx(&src.sheet, ctx) {
            let matched = rule
                .selectors
                .iter()
                .any(|s| node.matches_chain(s, ancestors));
            if !matched {
                continue;
            }
            let specificity = rule
                .selectors
                .iter()
                .filter(|s| node.matches_chain(s, ancestors))
                .map(|s| s.specificity())
                .max()
                .unwrap_or_default();
            for decl in &rule.declarations {
                let key = CascadeKey {
                    important: decl.important,
                    origin: src.origin,
                    specificity,
                    source,
                };
                source += 1;
                let entry = winners.get(&decl.property);
                if entry.is_none_or(|(k, _)| key > *k) {
                    winners.insert(
                        decl.property.clone(),
                        (
                            key,
                            Computed {
                                value: decl.value.clone(),
                                important: decl.important,
                                origin: src.origin,
                                span: Some(decl.span),
                            },
                        ),
                    );
                }
            }
        }
    }
    winners.into_iter().map(|(k, (_, c))| (k, c)).collect()
}

/// The environment a media query is evaluated against (04 §6).
#[derive(Clone, Debug)]
pub struct MediaContext {
    /// Window width (logical px).
    pub width: f64,
    /// Window height (logical px).
    pub height: f64,
    /// DPI scale factor.
    pub scale: f64,
    /// Platform: `windows|macos|linux|android|ios`.
    pub platform: String,
    /// Pointer: `mouse|touch`.
    pub pointer: String,
}

impl Default for MediaContext {
    fn default() -> Self {
        MediaContext {
            width: 800.0,
            height: 600.0,
            scale: 1.0,
            platform: "linux".into(),
            pointer: "mouse".into(),
        }
    }
}

/// Evaluate one media query against `ctx`.
pub fn eval_query(q: &ast::MediaQuery, ctx: &MediaContext) -> bool {
    use ast::MediaOp::*;
    let num = |v: &Value| match v {
        Value::Number(n, _) => Some(*n),
        _ => None,
    };
    match q.feature.as_str() {
        "width" | "height" | "scale" => {
            let lhs = match q.feature.as_str() {
                "width" => ctx.width,
                "height" => ctx.height,
                _ => ctx.scale,
            };
            let Some(rhs) = num(&q.value) else {
                return false;
            };
            match q.op {
                Eq => lhs == rhs,
                Lt => lhs < rhs,
                Gt => lhs > rhs,
                Le => lhs <= rhs,
                Ge => lhs >= rhs,
            }
        }
        "platform" => matches!(&q.value, Value::Keyword(k) if *k == ctx.platform),
        "pointer" => matches!(&q.value, Value::Keyword(k) if *k == ctx.pointer),
        _ => false,
    }
}

/// Whether all queries in a `@media (...) and (...)` hold.
pub fn eval_media(queries: &[ast::MediaQuery], ctx: &MediaContext) -> bool {
    queries.iter().all(|q| eval_query(q, ctx))
}

/// Resolve the cascade for `node` in media `ctx`: `@media` rules participate
/// only when their query matches.
pub fn resolve_media(
    sources: &[StyleSource],
    node: &NodeDesc,
    ctx: &MediaContext,
) -> HashMap<String, Computed> {
    let mut winners: HashMap<String, (CascadeKey, Computed)> = HashMap::new();
    let mut source = 0usize;
    for src in sources {
        for rule in rules_in_ctx(&src.sheet, ctx) {
            if !rule.selectors.iter().any(|s| node.matches(s)) {
                continue;
            }
            let specificity = rule
                .selectors
                .iter()
                .filter(|s| node.matches(s))
                .map(|s| s.specificity())
                .max()
                .unwrap_or_default();
            for decl in &rule.declarations {
                let key = CascadeKey {
                    important: decl.important,
                    origin: src.origin,
                    specificity,
                    source,
                };
                source += 1;
                if winners.get(&decl.property).is_none_or(|(k, _)| key > *k) {
                    winners.insert(
                        decl.property.clone(),
                        (
                            key,
                            Computed {
                                value: decl.value.clone(),
                                important: decl.important,
                                origin: src.origin,
                                span: Some(decl.span),
                            },
                        ),
                    );
                }
            }
        }
    }
    winners.into_iter().map(|(k, (_, c))| (k, c)).collect()
}

fn rules_in_ctx<'a>(sheet: &'a Stylesheet, ctx: &MediaContext) -> Vec<&'a Rule> {
    let mut out = Vec::new();
    for item in &sheet.items {
        match item {
            Item::Rule(r) => out.push(r),
            Item::Media(queries, rules) if eval_media(queries, ctx) => out.extend(rules.iter()),
            _ => {}
        }
    }
    out
}

/// Build the token table for `theme`: `@tokens` first, then the matching
/// `@theme` block overrides (theme-scoped names win, 04 §4).
pub fn tokens_for(sheet: &Stylesheet, theme: ThemeKind) -> Tokens {
    let mut t = Tokens::new();
    for item in &sheet.items {
        if let Item::Tokens(bindings) = item {
            for b in bindings {
                t.insert(b.name.clone(), b.value.clone());
            }
        }
    }
    for item in &sheet.items {
        if let Item::Theme(kind, bindings) = item {
            if *kind == theme {
                for b in bindings {
                    t.insert(b.name.clone(), b.value.clone());
                }
            }
        }
    }
    t
}
