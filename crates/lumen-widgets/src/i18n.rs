//! Internationalization (T5.3): Fluent-style message catalogs with `{arg}`
//! interpolation and plural selection, plus locale-aware number formatting.
//! Missing keys surface a `W0401` diagnostic instead of failing silently.
//!
//! RTL *layout* mirroring lives in `lumen-layout` (`mirror_rtl`) and is driven
//! by [`crate::Headless::set_rtl`]; locales whose [`Locale::is_rtl`] is true
//! should pair with it.

use lumen_core::{codes, Diagnostic};
use std::collections::HashMap;

/// A BCP-47-ish locale tag (e.g. `en`, `ar`, `ja`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Locale(pub String);

impl Locale {
    /// Construct from a tag.
    pub fn new(tag: impl Into<String>) -> Locale {
        Locale(tag.into())
    }

    /// Whether this locale is right-to-left (Arabic, Hebrew, Persian, Urdu).
    pub fn is_rtl(&self) -> bool {
        let lang = self.0.split(['-', '_']).next().unwrap_or("");
        matches!(lang, "ar" | "he" | "fa" | "ur")
    }

    /// The CLDR-ish plural category for `n` (English-like: `one` vs `other`;
    /// Arabic adds `zero`/`two`/`few`/`many`).
    pub fn plural_category(&self, n: i64) -> &'static str {
        let lang = self.0.split(['-', '_']).next().unwrap_or("");
        match lang {
            // Languages without plural inflection.
            "ja" | "zh" | "ko" | "th" | "vi" => "other",
            "ar" => match n {
                0 => "zero",
                1 => "one",
                2 => "two",
                n if (3..=10).contains(&(n % 100)) => "few",
                n if (11..=99).contains(&(n % 100)) => "many",
                _ => "other",
            },
            // English-like default.
            _ => {
                if n == 1 {
                    "one"
                } else {
                    "other"
                }
            }
        }
    }
}

/// A set of translated messages for one or more locales.
#[derive(Default)]
pub struct Catalog {
    messages: HashMap<Locale, HashMap<String, String>>,
    fallback: Option<Locale>,
}

impl Catalog {
    /// An empty catalog.
    pub fn new() -> Catalog {
        Catalog::default()
    }

    /// Set the fallback locale used when a key is missing in the active one.
    pub fn with_fallback(mut self, locale: Locale) -> Catalog {
        self.fallback = Some(locale);
        self
    }

    /// Add `key -> template` for `locale`. Templates use `{name}` placeholders
    /// and `{count, plural, one {…} other {…}}`-style selection (simplified).
    pub fn insert(&mut self, locale: &Locale, key: impl Into<String>, template: impl Into<String>) {
        self.messages
            .entry(locale.clone())
            .or_default()
            .insert(key.into(), template.into());
    }

    fn raw(&self, locale: &Locale, key: &str) -> Option<&str> {
        self.messages
            .get(locale)
            .and_then(|m| m.get(key))
            .or_else(|| {
                self.fallback
                    .as_ref()
                    .and_then(|f| self.messages.get(f).and_then(|m| m.get(key)))
            })
            .map(String::as_str)
    }

    /// Translate `key` in `locale`, interpolating `args`. A missing key returns
    /// `⟨key⟩` and a `W0401` diagnostic.
    pub fn translate(
        &self,
        locale: &Locale,
        key: &str,
        args: &[(&str, Arg)],
    ) -> (String, Option<Diagnostic>) {
        let Some(template) = self.raw(locale, key) else {
            return (
                format!("⟨{key}⟩"),
                Some(Diagnostic::new(
                    codes::W0401,
                    format!("missing translation `{key}` for `{}`", locale.0),
                )),
            );
        };
        (interpolate(template, locale, args), None)
    }

    /// Translate, discarding the diagnostic (convenience).
    pub fn t(&self, locale: &Locale, key: &str, args: &[(&str, Arg)]) -> String {
        self.translate(locale, key, args).0
    }
}

/// An interpolation argument.
#[derive(Clone, Copy)]
pub enum Arg<'a> {
    /// A string.
    Str(&'a str),
    /// An integer (also drives plural selection).
    Int(i64),
}

fn interpolate(template: &str, locale: &Locale, args: &[(&str, Arg)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        // Find the brace that matches `open` (tokens may nest, e.g. plurals).
        let mut depth = 0i32;
        let mut close = None;
        for (i, ch) in rest[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else {
            out.push_str(&rest[open..]);
            return out;
        };
        let token = &rest[open + 1..close];
        rest = &rest[close + 1..];
        out.push_str(&resolve_token(token, locale, args));
    }
    out.push_str(rest);
    out
}

fn resolve_token(token: &str, locale: &Locale, args: &[(&str, Arg)]) -> String {
    // `name` or `count, plural, one {x} other {y}`.
    if let Some((name, plural)) = token.split_once(", plural,") {
        let n = match lookup(name.trim(), args) {
            Some(Arg::Int(n)) => n,
            _ => 0,
        };
        let cat = locale.plural_category(n);
        return select_plural(plural, cat).replace('#', &format_number(n, locale));
    }
    match lookup(token.trim(), args) {
        Some(Arg::Str(s)) => s.to_string(),
        Some(Arg::Int(n)) => format_number(n, locale),
        None => format!("{{{token}}}"),
    }
}

fn lookup<'a>(name: &str, args: &'a [(&str, Arg)]) -> Option<Arg<'a>> {
    args.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

fn select_plural(body: &str, category: &str) -> String {
    // Parse `one {…} other {…}` selecting the matching branch (else `other`).
    let mut found_other = String::new();
    let mut rest = body;
    while let Some(brace) = rest.find('{') {
        let cat = rest[..brace]
            .trim()
            .trim_end_matches(|c: char| c.is_whitespace());
        let cat = cat.rsplit(char::is_whitespace).next().unwrap_or(cat);
        let Some(end) = rest[brace..].find('}') else {
            break;
        };
        let value = &rest[brace + 1..brace + end];
        if cat == category {
            return value.to_string();
        }
        if cat == "other" {
            found_other = value.to_string();
        }
        rest = &rest[brace + end + 1..];
    }
    found_other
}

/// Format an integer with the locale's grouping separator (comma for most,
/// none for locales that don't group, e.g. `ja`).
pub fn format_number(n: i64, locale: &Locale) -> String {
    let lang = locale.0.split(['-', '_']).next().unwrap_or("");
    let sep = match lang {
        "de" | "es" | "it" | "fr" => '.',
        _ => ',',
    };
    let neg = n < 0;
    let digits = n.unsigned_abs().to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(sep);
        }
        grouped.push(c);
    }
    if neg {
        format!("-{grouped}")
    } else {
        grouped
    }
}
