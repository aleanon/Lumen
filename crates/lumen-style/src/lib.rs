//! `lumen-style` — the `.lss` styling language: lexer, parser, cascade, and
//! (later) tokens/themes/animation (04).
//!
//! The grammar is CSS-like by design (ADR-016) for AI-author familiarity.
//! Parsing is total and produces structured diagnostics (E0101–E0104 with
//! spans); a stylesheet with any error is rejected atomically so hot reload can
//! keep the previous one live (04 §9).
#![warn(missing_docs)]

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod properties;

pub use ast::{
    Combinator, Compound, Declaration, Item, Part, Rule, Selector, Specificity, Stylesheet,
    ThemeKind, Unit, Value,
};
pub use parser::{has_errors, parse};
pub use properties::KNOWN_PROPERTIES;

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
    /// Whether `selector`'s rightmost compound matches this node.
    ///
    /// Ancestor combinators are not evaluated here (the cascade table operates
    /// on a single node); the full ancestor walk lives in the runtime styler.
    pub fn matches(&self, selector: &Selector) -> bool {
        let target = selector
            .rest
            .last()
            .map(|(_, c)| c)
            .unwrap_or(&selector.first);
        target.parts.iter().all(|p| self.matches_part(p))
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

/// The computed value of a property plus the cascade key that won it.
#[derive(Clone, Debug)]
pub struct Computed {
    /// The winning value.
    pub value: Value,
    /// Whether it was `!important`.
    pub important: bool,
    /// The origin it came from.
    pub origin: Origin,
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
/// source order. Returns the winning value per property.
pub fn resolve(sources: &[StyleSource], node: &NodeDesc) -> HashMap<String, Computed> {
    let mut winners: HashMap<String, (CascadeKey, Computed)> = HashMap::new();
    let mut source = 0usize;

    for src in sources {
        for rule in rules_of(&src.sheet) {
            let matched = rule.selectors.iter().any(|s| node.matches(s));
            if !matched {
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
                            },
                        ),
                    );
                }
            }
        }
    }
    winners.into_iter().map(|(k, (_, c))| (k, c)).collect()
}

/// All top-level rules of a stylesheet (media rules are flattened in; their
/// queries are evaluated by the runtime, not the cascade table).
fn rules_of(sheet: &Stylesheet) -> Vec<&Rule> {
    let mut out = Vec::new();
    for item in &sheet.items {
        match item {
            Item::Rule(r) => out.push(r),
            Item::Media(_, rules) => out.extend(rules.iter()),
            _ => {}
        }
    }
    out
}
