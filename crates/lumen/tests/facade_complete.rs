//! SD4: every type named in a facade-exported public signature must itself be
//! reachable **through the facade**.
//!
//! # Why this test exists
//!
//! `lumen` is the crate a consumer depends on; `lumen-widgets` is an
//! implementation crate. But `Element`'s builders take `lumen_style::Style`,
//! `Shadow` and `Dynamic<T>`, and none of those were re-exported — so
//! `css()`, `shadow()`, `bind_text()`, `bind_background()`, `bind_text_color()` and `bind_class()`
//! were **uncallable** by anyone depending only on `lumen`. The methods
//! compiled, documented and tested fine; they simply could not be named.
//!
//! It survived because nothing exercised it: every example that uses these APIs
//! imports `lumen_widgets::` directly, bypassing the facade entirely. A gap in
//! the facade is therefore invisible to the whole test suite by construction —
//! which is the same defect shape as the parity suite that self-skipped with no
//! adapter present, and the ABI hash that fingerprinted nothing.
//!
//! This file may import **only** from `lumen`. That restriction is the whole
//! point: adding a `use lumen_widgets::...` here to fix a failure would defeat
//! it. If a type is missing, re-export it from the facade instead.
#![deny(unused_imports)]

use lumen::layout::LayoutStyle;
use lumen::state::Runtime;
use lumen::style::Style;
use lumen::{widgets, Color, Dynamic, Element, Shadow, Signal};

/// Exercises every `Element` builder whose parameter type lives outside
/// `lumen`'s own root. Compiling *is* the assertion.
#[test]
fn element_builders_are_callable_through_the_facade() {
    let el = Element::default()
        .css(Style::new())
        .shadow(Shadow::soft())
        .style(LayoutStyle::default());
    assert_eq!(el.children.len(), 0);
}

/// The reactive-binding builders (F3/F5.2). `Dynamic<T>` is the parameter type
/// for all three, and it was the least reachable of the three gaps: it lives at
/// `lumen_core`'s root, and the facade re-exports that root only field by field.
#[test]
fn reactive_binding_builders_are_callable_through_the_facade() {
    let text: Dynamic<String> = Dynamic::new(|rt: &Runtime| {
        let s: Signal<i64> = rt.signal("facade-probe", || 0);
        format!("{}", s.get(rt))
    });
    let bg: Dynamic<Color> = Dynamic::new(|_: &Runtime| Color::BLACK);
    let classes: Dynamic<Vec<String>> = Dynamic::new(|_: &Runtime| vec!["a".into()]);

    let color: Dynamic<Color> = Dynamic::new(|_: &Runtime| Color::BLACK);
    let el = Element::default()
        .bind_text(text)
        .bind_background(bg)
        .bind_text_color(color)
        .bind_class(classes);
    assert!(el.dyn_text.is_some());
    assert!(el.dyn_bg.is_some());
    assert!(el.dyn_classes.is_some());
    assert!(el.rare.as_ref().is_some_and(|r| r.dyn_color.is_some()));
}

/// The theming helpers (`theme::card`, `theme::accent`, …) are a documented part
/// of the app-building surface and are used by the examples, but reached
/// `lumen_widgets` directly. A consumer on the facade could not name the module.
#[test]
fn theme_helpers_are_reachable_through_the_facade() {
    let _ = lumen::theme::accent();
    let _: Element = lumen::theme::card(widgets::text("body"));
}

/// E1: the statement-form + state-struct surface is reachable facade-only —
/// exactly what `lumen new` scaffolds. If this stops compiling, scaffolded
/// apps break before any user sees it.
#[test]
fn statement_form_and_state_struct_are_facade_complete() {
    use lumen::{App, Button, Label, Reactive, Stack};

    #[derive(Default, Reactive, serde::Serialize, serde::Deserialize)]
    #[serde(default)]
    struct S {
        n: i32,
    }

    let mut h = App::with_state(S::default(), |cx, s: &S| {
        let n = *s.n(cx);
        Stack::column(move |c| {
            c.child(Label::new(format!("n {n}")).id("n"));
            c.child(Stack::row(|_| {}).grow(1.0));
            c.child(
                Button::new("+")
                    .on_press(|rt| S::update_n(rt, |v| *v += 1))
                    .id("inc"),
            );
        })
        .centered()
        .shadow(lumen::Shadow::soft())
    })
    .run_headless(lumen::geometry::Size::new(200.0, 100.0));
    h.pump();
    S::set_n(h.runtime(), 5);
    h.pump();
    // The JSON projection is snapshot-gated; the lean leg still exercises the
    // whole build/write/patch path above.
    #[cfg(feature = "snapshot")]
    assert!(h.semantics_json().to_string().contains("n 5"));
}
