//! Procedural macros for Lumen (ADR-003 amendment, 2026-07-03).
//!
//! - [`stable_handler`] — the F2 handler-currency check (a retained handler may
//!   only capture stable `Copy` state, never an owned snapshot).
//! - [`text`] — the F3 binding sugar: `text!(cx, "Count: {count}")` builds a
//!   reactive text element whose string tracks the interpolated signals.

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{parse_macro_input, Expr, ExprClosure, Ident, LitStr, Token};

/// Wrap a click/activate handler closure (`Fn(&Runtime)`) with a compile-time
/// currency check.
///
/// Expands to the closure unchanged, behind an assertion that it is `Copy`. A
/// `Copy` closure can only have captured `Copy` values — reactive handles
/// (`Signal`/`Memo`), scalars, small `Copy` structs — never an owned snapshot
/// (`String`, `Vec`, `HashMap`, `Rc`) that would go stale once the handler is
/// retained across rebuilds (F2). Use it where a handler is built:
/// `widgets::button("x", stable_handler!(move |rt| count.update(rt, |c| *c += 1)))`.
///
/// The assertion wrapper carries the `Fn(&Runtime)` bound itself so the
/// closure's `for<'a> Fn(&'a Runtime)` (HRTB) inference is preserved — hence the
/// macro is specific to the click-handler signature (`lumen_core` must be in
/// scope, as it is throughout the widget crates and generated apps). Passing +
/// rejected examples are doctests on `lumen_widgets::stable_handler`.
///
/// **Scope.** This catches captured *owned* state. It does **not** catch a
/// captured `Copy` index into a collection (a `usize` is `Copy`) — that
/// staleness is a semantic property no type check can see; prefer capturing a
/// stable key or the reactive handle. Per-capture `Stable`-trait analysis is a
/// possible future tightening.
#[proc_macro]
pub fn stable_handler(input: TokenStream) -> TokenStream {
    let closure = parse_macro_input!(input as ExprClosure);
    let span = closure.span();
    // The `Fn(&Runtime)` bound on the assertion supplies the higher-ranked
    // signature, so the closure keeps its HRTB (a bare generic `fn(F) -> F`
    // would fix the argument lifetime too early). `Copy` is the currency check.
    // Named so the unsatisfied-bound error reads as a handler-currency
    // violation, pointed at the offending closure.
    quote_spanned! {span=>
        {
            fn __lumen_handler_captures_must_be_stable<__F>(f: __F) -> __F
            where
                __F: ::core::marker::Copy + ::core::ops::Fn(&::lumen_core::state::Runtime),
            {
                f
            }
            __lumen_handler_captures_must_be_stable(#closure)
        }
    }
    .into()
}

/// `text!(cx, "literal with {signal} holes")` → a reactive text element (F3).
///
/// Each `{name}` interpolates the current value of signal `name` in scope
/// (`name.get(rt)`), and `{name:spec}` applies a format spec. The macro emits a
/// [`Dynamic`](lumen_core::Dynamic) binding capturing those signals, so the text
/// re-evaluates when any of them changes, and records them as the node's deps.
/// With no holes it is just a static text element. `lumen_core` + `lumen_widgets`
/// must be in scope (as in the widget crates, examples, and generated apps).
///
/// ```ignore
/// text!(cx, "Count: {count}")           // tracks `count`
/// text!(cx, "{first} {last}")           // tracks two signals
/// text!(cx, "{ratio:.1}%")              // with a format spec
/// ```
#[proc_macro]
pub fn text(input: TokenStream) -> TokenStream {
    let TextInput { cx, fmt } = parse_macro_input!(input as TextInput);
    let span = fmt.span();
    let (rewritten, signals) = match parse_holes(&fmt.value()) {
        Ok(v) => v,
        Err(e) => {
            return syn::Error::new(span, e).to_compile_error().into();
        }
    };

    // No holes → a plain static text element (no binding, no deps).
    if signals.is_empty() {
        return quote_spanned! {span=>
            ::lumen_widgets::widgets::text(#fmt)
        }
        .into();
    }

    let fmt_lit = LitStr::new(&rewritten, span);
    let getters = signals.iter().map(|s| quote! { #s.get(__rt) });
    quote_spanned! {span=>
        {
            let __text_binding = ::lumen_core::Dynamic::new(move |__rt: &::lumen_core::Runtime| {
                ::std::format!(#fmt_lit, #(#getters),*)
            });
            let __text_initial = __text_binding.get((#cx).runtime());
            ::lumen_widgets::widgets::text(__text_initial).bind_text(__text_binding)
        }
    }
    .into()
}

/// `text!` input: a `BuildCx` expression, a comma, then the format literal.
struct TextInput {
    cx: Expr,
    fmt: LitStr,
}

impl Parse for TextInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let cx: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let fmt: LitStr = input.parse()?;
        Ok(TextInput { cx, fmt })
    }
}

/// Parse a format string into (positional format string, captured signal idents).
/// `{name}` → `{}` + `name`; `{name:spec}` → `{:spec}` + `name`; `{{`/`}}` are
/// literal braces. A hole's name must be a bare identifier.
fn parse_holes(s: &str) -> Result<(String, Vec<Ident>), String> {
    let mut out = String::new();
    let mut idents: Vec<Ident> = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push_str("{{");
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push_str("}}");
            }
            '{' => {
                let mut inner = String::new();
                for ic in chars.by_ref() {
                    if ic == '}' {
                        break;
                    }
                    inner.push(ic);
                }
                let (name, spec) = match inner.split_once(':') {
                    Some((n, sp)) => (n.trim(), Some(sp)),
                    None => (inner.trim(), None),
                };
                if name.is_empty() {
                    return Err(
                        "empty `{}` hole: `text!` needs a signal name, e.g. `{count}`".to_string(),
                    );
                }
                let ident = syn::parse_str::<Ident>(name)
                    .map_err(|_| format!("`{name}` is not a valid signal identifier in a hole"))?;
                idents.push(ident);
                match spec {
                    Some(sp) => {
                        out.push_str("{:");
                        out.push_str(sp);
                        out.push('}');
                    }
                    None => out.push_str("{}"),
                }
            }
            '}' => {
                return Err("unmatched `}` in format string (use `}}` for a literal)".to_string())
            }
            other => out.push(other),
        }
    }
    Ok((out, idents))
}
