//! Procedural macros for Lumen (ADR-003 amendment, 2026-07-03).
//!
//! Currently one macro: [`stable_handler`], the F2 handler-currency check. In a
//! retained view (the fine-grained model, `docs/plan-fine-grained-view.md`) an
//! event handler is attached to a node **once** and persists across rebuilds, so
//! it must not close over transient build-time values — a captured `String`/`Vec`
//! snapshot of state goes stale. `stable_handler!` enforces this at compile time
//! by requiring the closure to be `Copy`: it may capture reactive identity
//! (`Signal`/`Memo` handles are `Copy`) and scalars, but not owned collections.

use proc_macro::TokenStream;
use quote::quote_spanned;
use syn::spanned::Spanned;
use syn::{parse_macro_input, ExprClosure};

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
