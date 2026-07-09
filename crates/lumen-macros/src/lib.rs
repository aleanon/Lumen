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
/// **Every** `{name}` hole is a signal read (`name.get(rt)`); non-signal values
/// must be baked into the literal or interpolated via `bind_text` directly.
/// `{name:spec}` applies a format spec. The macro emits a
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

/// `#[lumen_test::test]` (05 §1): turn `async fn t(app: TestApp) { … }` into a
/// synchronous `#[test]` that constructs the app under test and drives the
/// body on the harness executor (`lumen_test::block_on`).
///
/// Options (comma-separated):
/// - `size(w, h)` — window size in logical px (default `800, 600`)
/// - `scale(f)` — HiDPI factor (default `1.0`)
/// - `theme(light | dark | high-contrast)` — default `light`
/// - `app(expr)` — the `App` constructor expression; defaults to
///   `main_app()`, which must be in scope (`use my_app::main_app;` — the
///   `lumen new` convention)
/// - `platform(name)` — marks the test `#[ignore]` (platform runners are
///   orchestrated externally; run explicitly with `--ignored`)
///
/// ```ignore
/// #[lumen_test::test(size(390, 844), theme(dark))]
/// async fn mobile_checkout(mut app: TestApp) {
///     app.pump_until_idle().await;
///     app.locator("#buy").click().await.unwrap();
/// }
/// ```
#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as syn::ItemFn);
    if func.sig.asyncness.is_none() {
        return syn::Error::new(
            func.sig.span(),
            "#[lumen_test::test] requires an `async fn`",
        )
        .to_compile_error()
        .into();
    }
    if func.sig.inputs.len() != 1 {
        return syn::Error::new(
            func.sig.span(),
            "#[lumen_test::test] body takes exactly one parameter: `app: TestApp`",
        )
        .to_compile_error()
        .into();
    }

    let mut width = 800.0f64;
    let mut height = 600.0f64;
    let mut scale = 1.0f64;
    let mut theme = String::from("light");
    let mut app_expr: Expr = syn::parse_quote!(main_app());
    let mut platform: Option<String> = None;

    if !attr.is_empty() {
        let metas = parse_macro_input!(
            attr with syn::punctuated::Punctuated::<syn::Meta, Token![,]>::parse_terminated
        );
        for meta in metas {
            let syn::Meta::List(list) = meta else {
                return syn::Error::new(
                    meta.span(),
                    "expected `name(…)` options: size(w, h), scale(f), theme(t), app(expr), platform(p)",
                )
                .to_compile_error()
                .into();
            };
            let name = list
                .path
                .get_ident()
                .map(ToString::to_string)
                .unwrap_or_default();
            match name.as_str() {
                "size" => {
                    let nums: syn::punctuated::Punctuated<syn::Lit, Token![,]> =
                        match list.parse_args_with(syn::punctuated::Punctuated::parse_terminated) {
                            Ok(n) => n,
                            Err(e) => return e.to_compile_error().into(),
                        };
                    let vals: Vec<f64> = nums.iter().filter_map(lit_f64).collect();
                    if vals.len() != 2 {
                        return syn::Error::new(list.span(), "size takes two numbers: size(w, h)")
                            .to_compile_error()
                            .into();
                    }
                    width = vals[0];
                    height = vals[1];
                }
                "scale" => {
                    let lit: syn::Lit = match list.parse_args() {
                        Ok(l) => l,
                        Err(e) => return e.to_compile_error().into(),
                    };
                    match lit_f64(&lit) {
                        Some(v) => scale = v,
                        None => {
                            return syn::Error::new(list.span(), "scale takes a number")
                                .to_compile_error()
                                .into()
                        }
                    }
                }
                "theme" => theme = list.tokens.to_string().replace(' ', ""),
                "app" => {
                    app_expr = match list.parse_args() {
                        Ok(e) => e,
                        Err(e) => return e.to_compile_error().into(),
                    }
                }
                "platform" => platform = Some(list.tokens.to_string()),
                other => {
                    return syn::Error::new(
                        list.span(),
                        format!("unknown option `{other}` (size/scale/theme/app/platform)"),
                    )
                    .to_compile_error()
                    .into();
                }
            }
        }
    }

    let name = &func.sig.ident;
    let vis = &func.vis;
    let theme_lit = LitStr::new(&theme, proc_macro2::Span::call_site());
    let ignore_attr = platform.map(|p| {
        let reason = LitStr::new(
            &format!("platform runner required: {p}"),
            proc_macro2::Span::call_site(),
        );
        quote!(#[ignore = #reason])
    });

    quote! {
        #[test]
        #ignore_attr
        #vis fn #name() {
            #func
            ::lumen_test::block_on(async {
                let __app = ::lumen_test::TestApp::with_config(
                    #app_expr,
                    ::lumen_test::Size::new(#width, #height),
                    #scale,
                    #theme_lit,
                );
                #name(__app).await;
            });
        }
    }
    .into()
}

/// A numeric literal (int or float) as `f64`.
fn lit_f64(lit: &syn::Lit) -> Option<f64> {
    match lit {
        syn::Lit::Int(i) => i.base10_parse::<f64>().ok(),
        syn::Lit::Float(f) => f.base10_parse::<f64>().ok(),
        _ => None,
    }
}
