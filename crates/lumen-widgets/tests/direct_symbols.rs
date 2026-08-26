//! Step 2 — identity without allocation.
//!
//! Attribution showed the remaining per-node cost was not where "intern the
//! strings" assumes. A short `StableId` inlines into its `SmolStr` and costs
//! nothing to store; the 2.00 allocations per node were `format!("row{i}")` at
//! the *call site*. So ids need a structured form (no string minted), and
//! classes need interning (the `String` and its `Vec` are both real).
//!
//! Speed is worthless if the cascade stops matching, so these tests check that
//! the allocation-free forms are *indistinguishable* to selectors, which is the
//! only thing that makes them adoptable.

use lumen_core::semantics::Role;
use lumen_layout::{Dim, LayoutStyle};
use lumen_widgets::direct::{NodeId, StyleEnv, Sym, Symbols, TreeSink, VisualState};

fn sink(src: &str) -> TreeSink {
    TreeSink::new().with_styles(
        StyleEnv::from_source(src).expect("parses"),
        VisualState::default(),
    )
}

#[test]
fn an_interned_class_matches_the_same_rules_as_a_string_one() {
    let mut a = sink(".row { width: 300px; }");
    let n1 = {
        let d = a.node(None, Role::Group).class("row").resolve();
        let i = d.index();
        d.end(&LayoutStyle::default(), &[], false);
        i
    };

    let mut b = sink(".row { width: 300px; }");
    let n2 = {
        let d = b.node(None, Role::Group).class_static("row").resolve();
        let i = d.index();
        d.end(&LayoutStyle::default(), &[], false);
        i
    };

    assert_eq!(a.meta[&n1].layout_style.width, Dim::px(300.0));
    assert_eq!(
        b.meta[&n2].layout_style.width,
        Dim::px(300.0),
        "the interned class matched the same rule"
    );
}

#[test]
fn a_structured_id_matches_an_id_selector() {
    // `id_at("row", 5)` must be indistinguishable from `.id("row5")` — the
    // string is rendered only when a selector asks.
    let mut s = sink("#row5 { width: 220px; } #row6 { width: 999px; }");
    let hit = {
        let d = s.node(None, Role::Group).id_at("row", 5).resolve();
        let i = d.index();
        d.end(&LayoutStyle::default(), &[], false);
        i
    };
    let miss = {
        let d = s.node(None, Role::Group).id_at("row", 7).resolve();
        let i = d.index();
        d.end(&LayoutStyle::default(), &[], false);
        i
    };
    assert_eq!(s.meta[&hit].layout_style.width, Dim::px(220.0));
    assert_eq!(
        s.meta[&miss].layout_style.width,
        Dim::Auto,
        "row7 matched nothing, so the index is really part of the identity"
    );
}

#[test]
fn interning_is_stable_and_deduplicates() {
    let mut syms = Symbols::default();
    let a = syms.intern_static("row");
    let b = syms.intern_static("row");
    let c = syms.intern_static("cell");
    assert_eq!(a, b, "the same text interns to the same symbol");
    assert_ne!(a, c);
    assert_eq!(syms.len(), 2, "and only distinct strings take a slot");
    assert_eq!(syms.text(a), "row");
    assert_eq!(syms.text(c), "cell");
}

#[test]
fn a_dynamic_class_still_works_and_costs_once() {
    // A class computed at runtime cannot be `&'static`, so it allocates — but
    // only on first sight, not once per node per frame.
    let mut syms = Symbols::default();
    let first = syms.intern(&format!("gen-{}", 1));
    let again = syms.intern("gen-1");
    assert_eq!(first, again, "a borrowed string interns to the same symbol");
    assert_eq!(syms.len(), 1);
    assert_eq!(syms.text(first), "gen-1");
}

#[test]
fn the_class_set_spills_correctly_past_its_inline_capacity() {
    // Three inline covers real nodes; the spill has to be right, not just fast.
    let mut s = TreeSink::new();
    let syms: Vec<_> = ["a", "b", "c", "d", "e"]
        .iter()
        .map(|t| s.sym(t))
        .collect();
    let n = {
        let mut d = s.node(None, Role::Group);
        for k in &syms {
            d = d.class_sym(*k);
        }
        let d = d.resolve();
        let i = d.index();
        d.end(&LayoutStyle::default(), &[], false);
        i
    };
    let got: Vec<Sym> = s.meta[&n].class_syms.iter().collect();
    assert_eq!(got.len(), 5, "all five survived the spill");
    for (i, k) in syms.iter().enumerate() {
        assert_eq!(got[i], *k, "and in order");
    }
}

#[test]
fn a_structured_id_renders_the_string_a_selector_expects() {
    let mut syms = Symbols::default();
    let row = syms.intern_static("row");
    assert_eq!(NodeId::at(row, 12).to_string_in(&syms), "row12");
    assert_eq!(NodeId::name(row).to_string_in(&syms), "row");
}
