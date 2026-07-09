//! The `.lss` recursive-descent parser (04 §1–§2).
//!
//! Produces an AST plus structured diagnostics (E0101 syntax, E0102 unknown
//! property with did-you-mean, E0104 unknown `$token`). Parsing never panics; a
//! stylesheet with any error is rejected atomically by the caller (04 §9).

use crate::ast::*;
use crate::lexer::{lex, Tk, Token};
use crate::properties::KNOWN_PROPERTIES;
use lumen_core::diagnostics::{codes, Diagnostic, Severity, SourceSpan};
use lumen_core::Color;
use std::collections::HashSet;

/// Parse `src` (named `file` for diagnostics). Returns the stylesheet and any
/// diagnostics; the stylesheet is only valid if no `severity == Error`
/// diagnostics are present.
pub fn parse(file: &str, src: &str) -> (Stylesheet, Vec<Diagnostic>) {
    let mut p = Parser {
        toks: lex(src),
        pos: 0,
        file: file.to_string(),
        diags: Vec::new(),
        defined: HashSet::new(),
        var_uses: Vec::new(),
    };
    let mut sheet = p.stylesheet();
    p.validate_vars();
    expand_nested(&mut sheet);
    (sheet, p.diags)
}

/// B.1: flatten nested `&` rules into standalone rules so the cascade and
/// (chain-)matcher see plain selectors — `#x { &:hovered { … } }` becomes
/// `#x:hovered { … }`, and `& > .thumb` becomes `#x > .thumb`. Specificity
/// then falls out of the synthesized selector. Applied to every rule
/// container (top level and `@media` blocks).
fn expand_nested(sheet: &mut Stylesheet) {
    fn expand_rules(rules: &mut Vec<Rule>) {
        let mut flattened = Vec::new();
        for rule in rules.iter_mut() {
            for nested in rule.nested.drain(..) {
                let selectors = rule
                    .selectors
                    .iter()
                    .map(|s| {
                        let mut s = s.clone();
                        if nested.child {
                            s.rest.push((
                                crate::ast::Combinator::Child,
                                crate::ast::Compound {
                                    parts: nested.parts.clone(),
                                },
                            ));
                        } else {
                            let target = s.rest.last_mut().map(|(_, c)| c).unwrap_or(&mut s.first);
                            target.parts.extend(nested.parts.iter().cloned());
                        }
                        s
                    })
                    .collect();
                flattened.push(Rule {
                    selectors,
                    declarations: nested.declarations,
                    nested: Vec::new(),
                });
            }
        }
        rules.extend(flattened);
    }
    let mut top = Vec::new();
    for item in &mut sheet.items {
        match item {
            Item::Rule(r) => top.push(r),
            Item::Media(_, rules) => expand_rules(rules),
            _ => {}
        }
    }
    // Top-level rules live as individual items — expand into new items.
    let mut new_rules = Vec::new();
    for rule in top {
        let mut single = vec![std::mem::replace(
            rule,
            Rule {
                selectors: Vec::new(),
                declarations: Vec::new(),
                nested: Vec::new(),
            },
        )];
        expand_rules(&mut single);
        let mut it = single.into_iter();
        *rule = it.next().expect("original rule");
        new_rules.extend(it);
    }
    sheet.items.extend(new_rules.into_iter().map(Item::Rule));
}

/// The expected value type for E0103 validation (B.7a) — covers the
/// *applied* property set (04 §10 "rendered"/"applied"); parse-only
/// properties are unvalidated until they gain runtime meaning.
fn expected_value_type(property: &str) -> Option<&'static str> {
    Some(match property {
        "background" | "color" | "border-color" => "color",
        "width" | "height" | "gap" | "padding" | "margin" | "border-radius" | "font-size"
        | "border-width" => "length",
        "opacity" | "font-weight" | "line-height" => "number",
        "display" | "flex-direction" => "keyword",
        _ => return None,
    })
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    file: String,
    diags: Vec<Diagnostic>,
    defined: HashSet<String>,
    var_uses: Vec<(String, Span)>,
}

impl Parser {
    fn cur(&self) -> &Tk {
        &self.toks[self.pos].kind
    }
    fn span(&self) -> Span {
        self.toks[self.pos].span
    }
    fn ws_before(&self) -> bool {
        self.toks[self.pos].ws_before
    }
    fn bump(&mut self) -> Tk {
        let k = self.toks[self.pos].kind.clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        k
    }
    fn at(&self, k: &Tk) -> bool {
        self.cur() == k
    }
    fn eof(&self) -> bool {
        matches!(self.cur(), Tk::Eof)
    }

    fn err(&mut self, code: &'static str, msg: impl Into<String>) {
        let span = self.span();
        self.err_at(code, msg, span);
    }
    fn err_at(&mut self, code: &'static str, msg: impl Into<String>, span: Span) {
        self.diags
            .push(Diagnostic::new(code, msg).with_span(SourceSpan {
                file: self.file.clone(),
                line: span.line,
                col: span.col,
            }));
    }

    /// Consume `k` or emit E0101 and recover.
    fn expect(&mut self, k: Tk, what: &str) {
        if self.at(&k) {
            self.bump();
        } else {
            self.err(codes::E0101, format!("expected {what}"));
        }
    }

    fn stylesheet(&mut self) -> Stylesheet {
        let mut items = Vec::new();
        while !self.eof() {
            let before = self.pos;
            if let Some(item) = self.item() {
                items.push(item);
            }
            if self.pos == before {
                // No progress (unexpected token): report and skip to recover.
                self.err(codes::E0101, "unexpected token");
                self.bump();
            }
        }
        Stylesheet { items }
    }

    fn item(&mut self) -> Option<Item> {
        if let Tk::At(kw) = self.cur().clone() {
            match kw.as_str() {
                "tokens" => return Some(self.tokens_block()),
                "theme" => return Some(self.theme_block()),
                "keyframes" => return Some(self.keyframes()),
                "media" => return Some(self.media()),
                other => {
                    self.err(codes::E0101, format!("unknown at-rule `@{other}`"));
                    self.bump();
                    return None;
                }
            }
        }
        Some(Item::Rule(self.rule()?))
    }

    fn tokens_block(&mut self) -> Item {
        self.bump(); // @tokens
        self.expect(Tk::LBrace, "`{`");
        let bindings = self.bindings(true);
        self.expect(Tk::RBrace, "`}`");
        Item::Tokens(bindings)
    }

    fn theme_block(&mut self) -> Item {
        self.bump(); // @theme
        let kind = match self.cur().clone() {
            Tk::Ident(s) if s == "light" => ThemeKind::Light,
            Tk::Ident(s) if s == "dark" => ThemeKind::Dark,
            Tk::Ident(s) if s == "high-contrast" => ThemeKind::HighContrast,
            _ => {
                self.err(codes::E0101, "expected light|dark|high-contrast");
                ThemeKind::Light
            }
        };
        if matches!(self.cur(), Tk::Ident(_)) {
            self.bump();
        }
        self.expect(Tk::LBrace, "`{`");
        let bindings = self.bindings(true);
        self.expect(Tk::RBrace, "`}`");
        Item::Theme(kind, bindings)
    }

    /// Parse `ident: value;` bindings; if `define`, record names as tokens.
    fn bindings(&mut self, define: bool) -> Vec<Binding> {
        let mut out = Vec::new();
        while !self.at(&Tk::RBrace) && !self.eof() {
            let span = self.span();
            let name = match self.cur().clone() {
                Tk::Ident(s) => {
                    self.bump();
                    s
                }
                _ => {
                    self.err(codes::E0101, "expected token name");
                    self.bump();
                    continue;
                }
            };
            self.expect(Tk::Colon, "`:`");
            let value = self.value();
            self.expect(Tk::Semi, "`;`");
            if define {
                self.defined.insert(name.clone());
            }
            out.push(Binding { name, value, span });
        }
        out
    }

    fn keyframes(&mut self) -> Item {
        self.bump(); // @keyframes
        let name = self.ident_or_err("keyframes name");
        self.expect(Tk::LBrace, "`{`");
        let mut stops = Vec::new();
        while !self.at(&Tk::RBrace) && !self.eof() {
            let pct = match self.cur().clone() {
                Tk::Num(n, _) => {
                    self.bump();
                    if self.at(&Tk::Percent) {
                        self.bump();
                    }
                    n as f32
                }
                _ => {
                    self.err(codes::E0101, "expected keyframe percent");
                    self.bump();
                    continue;
                }
            };
            self.expect(Tk::LBrace, "`{`");
            let decls = self.declarations();
            self.expect(Tk::RBrace, "`}`");
            stops.push((pct, decls));
        }
        self.expect(Tk::RBrace, "`}`");
        Item::Keyframes(Keyframes { name, stops })
    }

    fn media(&mut self) -> Item {
        self.bump(); // @media
        let mut queries = Vec::new();
        loop {
            self.expect(Tk::LParen, "`(`");
            let feature = self.ident_or_err("media feature");
            let op = match self.cur() {
                Tk::Colon => MediaOp::Eq,
                Tk::Lt => MediaOp::Lt,
                Tk::Gt => MediaOp::Gt,
                Tk::Le => MediaOp::Le,
                Tk::Ge => MediaOp::Ge,
                _ => {
                    self.err(codes::E0101, "expected comparison operator");
                    MediaOp::Eq
                }
            };
            self.bump();
            let value = self.value();
            self.expect(Tk::RParen, "`)`");
            queries.push(MediaQuery { feature, op, value });
            if let Tk::Ident(s) = self.cur() {
                if s == "and" {
                    self.bump();
                    continue;
                }
            }
            break;
        }
        self.expect(Tk::LBrace, "`{`");
        let mut rules = Vec::new();
        while !self.at(&Tk::RBrace) && !self.eof() {
            let before = self.pos;
            if let Some(r) = self.rule() {
                rules.push(r);
            }
            if self.pos == before {
                self.bump();
            }
        }
        self.expect(Tk::RBrace, "`}`");
        Item::Media(queries, rules)
    }

    fn rule(&mut self) -> Option<Rule> {
        let selectors = self.selector_list();
        if selectors.is_empty() {
            return None;
        }
        self.expect(Tk::LBrace, "`{`");
        let mut declarations = Vec::new();
        let mut nested = Vec::new();
        while !self.at(&Tk::RBrace) && !self.eof() {
            if self.at(&Tk::Amp) {
                nested.push(self.nested_rule());
            } else {
                let before = self.pos;
                if let Some(d) = self.declaration() {
                    declarations.push(d);
                }
                if self.pos == before {
                    self.bump();
                }
            }
        }
        self.expect(Tk::RBrace, "`}`");
        Some(Rule {
            selectors,
            declarations,
            nested,
        })
    }

    fn nested_rule(&mut self) -> NestedRule {
        self.bump(); // &
                     // B.1: `& > part+` nests a *child* compound (`slider > .thumb`).
        let child = if self.at(&Tk::Gt) {
            self.bump();
            true
        } else {
            false
        };
        let mut parts = Vec::new();
        while let Some(part) = self.part() {
            parts.push(part);
        }
        self.expect(Tk::LBrace, "`{`");
        let decls = self.declarations();
        self.expect(Tk::RBrace, "`}`");
        NestedRule {
            parts,
            child,
            declarations: decls,
        }
    }

    fn selector_list(&mut self) -> Vec<Selector> {
        let mut sels = Vec::new();
        loop {
            if let Some(s) = self.selector() {
                sels.push(s);
            }
            if self.at(&Tk::Comma) {
                self.bump();
                continue;
            }
            break;
        }
        sels
    }

    fn selector(&mut self) -> Option<Selector> {
        let span = self.span();
        let first = self.compound();
        if first.parts.is_empty() {
            return None;
        }
        let mut rest = Vec::new();
        loop {
            let comb = if self.at(&Tk::Gt) {
                self.bump();
                Combinator::Child
            } else if self.ws_before()
                && !self.at(&Tk::LBrace)
                && !self.at(&Tk::Comma)
                && !self.eof()
                && self.starts_compound()
            {
                Combinator::Descendant
            } else {
                break;
            };
            let c = self.compound();
            if c.parts.is_empty() {
                break;
            }
            rest.push((comb, c));
        }
        Some(Selector { first, rest, span })
    }

    fn starts_compound(&self) -> bool {
        matches!(
            self.cur(),
            Tk::Hash(_) | Tk::Dot | Tk::Colon | Tk::Star | Tk::Ident(_)
        )
    }

    fn compound(&mut self) -> Compound {
        let mut parts = Vec::new();
        // A compound is a run of parts not separated by whitespace.
        loop {
            if !parts.is_empty() && self.ws_before() {
                break;
            }
            match self.part() {
                Some(p) => parts.push(p),
                None => break,
            }
        }
        Compound { parts }
    }

    fn part(&mut self) -> Option<Part> {
        match self.cur().clone() {
            Tk::Hash(s) => {
                self.bump();
                Some(Part::Id(s))
            }
            Tk::Dot => {
                self.bump();
                Some(Part::Class(self.ident_or_err("class name")))
            }
            Tk::Colon => {
                self.bump();
                Some(Part::State(self.ident_or_err("state name")))
            }
            Tk::Star => {
                self.bump();
                Some(Part::Any)
            }
            Tk::Ident(s) if !s.is_empty() => {
                self.bump();
                Some(Part::Type(s))
            }
            _ => None,
        }
    }

    fn declarations(&mut self) -> Vec<Declaration> {
        let mut out = Vec::new();
        while !self.at(&Tk::RBrace) && !self.eof() {
            let before = self.pos;
            if let Some(d) = self.declaration() {
                out.push(d);
            }
            if self.pos == before {
                self.bump();
            }
        }
        out
    }

    fn declaration(&mut self) -> Option<Declaration> {
        let span = self.span();
        let property = match self.cur().clone() {
            Tk::Ident(s) if !s.is_empty() => {
                self.bump();
                s
            }
            _ => {
                self.err(codes::E0101, "expected property name");
                return None;
            }
        };
        if !KNOWN_PROPERTIES.contains(&property.as_str()) {
            let hint = did_you_mean(&property, KNOWN_PROPERTIES);
            let msg = match hint {
                Some(h) => format!("unknown property `{property}`; did you mean `{h}`?"),
                None => format!("unknown property `{property}`"),
            };
            self.err_at(codes::E0102, msg, span);
        }
        self.expect(Tk::Colon, "`:`");
        let value = self.value();
        let important = if self.at(&Tk::Bang) {
            self.bump();
            if let Tk::Ident(s) = self.cur().clone() {
                if s == "important" {
                    self.bump();
                }
            }
            true
        } else {
            false
        };
        self.expect(Tk::Semi, "`;`");
        // B.7a: parse-time type validation for the applied property set —
        // a mismatch is E0103 with the expected type (04 §9; the code was
        // defined-but-dead until now, and `apply()` silently ignored bad
        // values). `$token`/function/list values pass through (resolved or
        // interpreted later).
        if let Some(expected) = expected_value_type(&property) {
            let ok = match (&value, expected) {
                (Value::Var(_) | Value::Function(..) | Value::List(_), _) => true,
                (Value::Color(_), "color") => true,
                (Value::Number(..), "length") | (Value::Number(..), "number") => true,
                (Value::Keyword(k), "length") => k == "auto",
                (Value::Keyword(_), "keyword") => true,
                _ => false,
            };
            if !ok {
                self.err_at(
                    codes::E0103,
                    format!("`{property}` expects a {expected}, got `{value:?}`"),
                    span,
                );
            }
        }
        Some(Declaration {
            property,
            value,
            important,
            span,
        })
    }

    /// Parse a value: a run of atoms until `;`, `}`, `!`, `)`, or `,`.
    fn value(&mut self) -> Value {
        let mut atoms = Vec::new();
        loop {
            match self.cur().clone() {
                Tk::Semi | Tk::RBrace | Tk::Bang | Tk::RParen | Tk::Eof => break,
                Tk::Comma => {
                    self.bump();
                    continue;
                }
                _ => {}
            }
            match self.value_atom() {
                Some(a) => atoms.push(a),
                None => break,
            }
        }
        match atoms.len() {
            0 => {
                self.err(codes::E0101, "expected a value");
                Value::Keyword(String::new())
            }
            1 => atoms.pop().unwrap(),
            _ => Value::List(atoms),
        }
    }

    fn value_atom(&mut self) -> Option<Value> {
        match self.cur().clone() {
            Tk::Num(n, u) => {
                self.bump();
                Some(Value::Number(n, u))
            }
            Tk::Hash(h) => {
                let span = self.span();
                self.bump();
                match Color::from_hex(&format!("#{h}")) {
                    Ok(c) => Some(Value::Color(c)),
                    Err(_) => {
                        self.err_at(codes::E0101, format!("invalid color `#{h}`"), span);
                        Some(Value::Keyword(h))
                    }
                }
            }
            Tk::Var(v) => {
                let span = self.span();
                self.bump();
                self.var_uses.push((v.clone(), span));
                Some(Value::Var(v))
            }
            Tk::Str(s) => {
                self.bump();
                Some(Value::Str(s))
            }
            Tk::Ident(name) if !name.is_empty() => {
                self.bump();
                if self.at(&Tk::LParen) {
                    self.bump();
                    let mut args = Vec::new();
                    while !self.at(&Tk::RParen) && !self.eof() {
                        let before = self.pos;
                        args.push(self.value());
                        if self.at(&Tk::Comma) {
                            self.bump();
                        }
                        if self.pos == before {
                            break;
                        }
                    }
                    self.expect(Tk::RParen, "`)`");
                    Some(Value::Function(name, args))
                } else {
                    Some(Value::Keyword(name))
                }
            }
            _ => None,
        }
    }

    fn ident_or_err(&mut self, what: &str) -> String {
        match self.cur().clone() {
            Tk::Ident(s) if !s.is_empty() => {
                self.bump();
                s
            }
            _ => {
                self.err(codes::E0101, format!("expected {what}"));
                String::new()
            }
        }
    }

    fn validate_vars(&mut self) {
        let uses = std::mem::take(&mut self.var_uses);
        for (name, span) in uses {
            if !self.defined.contains(&name) {
                let defined: Vec<&str> = self.defined.iter().map(|s| s.as_str()).collect();
                let hint = did_you_mean(&name, &defined);
                let msg = match hint {
                    Some(h) => format!("unknown token `${name}`; did you mean `${h}`?"),
                    None => format!("unknown token `${name}`"),
                };
                self.err_at(codes::E0104, msg, span);
            }
        }
    }
}

/// Whether any diagnostic is an error (the stylesheet must be rejected).
pub fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.severity == Severity::Error)
}

/// Closest candidate within Levenshtein distance 2 (for did-you-mean hints).
fn did_you_mean<'a>(word: &str, candidates: &[&'a str]) -> Option<&'a str> {
    candidates
        .iter()
        .map(|c| (*c, levenshtein(word, c)))
        .filter(|&(_, d)| d <= 2)
        .min_by_key(|&(_, d)| d)
        .map(|(c, _)| c)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}
