//! A.3.2 (docs/plan-retained-pipeline.md): the memo-hit **copy-forward** path.
//! When `cx.scope` reports a hit, the runtime does not re-lower its subtree —
//! `copy_span`/`copy_node` move the retained meta, styles and layout styles
//! across from the previous build and re-derive only what is host state
//! (interaction flags, the taffy node, the parent link).
//!
//! This file exists because that path had **no dedicated coverage**: it was
//! created empty in July 2025 and stayed 1 byte. These tests pin the
//! *behaviour* (what a copied span must look like from the outside) rather
//! than any single line, because F2 rewrites how spans carry their state.
//!
//! Ablations run when they were written, to check they bite:
//!
//! | ablation | result |
//! |---|---|
//! | `copy_span` always returns `None` (memo hits re-lowered) | **3/3 fail** |
//! | nested span records not remapped onto the copied nodes | **1/3 fail** (the nested test) |
//! | `copy_node`'s interaction-flag refresh removed | 0/3 fail — see below |
//!
//! That last row is deliberate, not a gap in the tests. `restyle_visual`
//! (A.5a) refreshes interaction flags on the live tree the moment pointer or
//! focus state changes, so `prev_tree` already carries correct flags by the
//! time any rebuild copies a span — `copy_node`'s refresh is defensive
//! redundancy on today's code paths and is not independently observable from
//! outside. It is still the right thing for `copy_node` to do (it must not
//! depend on another pass having run first), and these tests will catch it if
//! F2 ever makes a copied span the *only* thing carrying that state.

use std::cell::Cell;
use std::rc::Rc;

use kurbo::{Point, Size};
use lumen_core::events::{Event, PointerEvent};
use lumen_core::identity::ScopePath;
use lumen_core::state::Signal;
use lumen_widgets::{center, col, widgets, App};

fn bg_of(styles: &serde_json::Value) -> Option<String> {
    styles
        .get("background")?
        .get("value")?
        .as_str()
        .map(str::to_string)
}

/// Build: a signal-driven label *outside* a scope (so writing it forces a
/// rebuild) and a button *inside* the scope (so the button rides the copy
/// path). Returns the harness and the scope's run counter.
fn app_with_button_inside_a_scope() -> (lumen_widgets::Headless, Rc<Cell<u32>>) {
    let runs = Rc::new(Cell::new(0u32));
    let runs_outer = runs.clone();
    let h = App::new(move |cx| {
        let tick: Signal<usize> = cx.signal("tick", || 0usize);
        let n = tick.get(cx.runtime());
        let runs = runs_outer.clone();
        col![
            widgets::text(format!("tick {n}")),
            cx.scope("card", move |_cx| {
                runs.set(runs.get() + 1);
                col![widgets::button("Hover me", |_| {}).id("b")]
            }),
        ]
    })
    .stylesheet(
        "#b { background: #00ff00ff; } \
         #b:hovered { background: #ff0000ff; }",
    )
    .run_headless(Size::new(300.0, 200.0));
    (h, runs)
}

/// The premise every other test here rests on: a memo hit really is *copied*,
/// not quietly re-lowered. Without this, a regression that disabled the copy
/// path entirely would leave the rest of the file passing vacuously.
#[test]
fn a_memo_hit_span_is_copied_not_relowered() {
    let (mut h, runs) = app_with_button_inside_a_scope();
    h.pump();
    let first = runs.get();
    assert!(first >= 1, "the scope ran on the first build");

    // Write a signal the scope does not read: the frame rebuilds, the scope
    // hits its memo, and its span is copied forward.
    let tick: Signal<usize> = h.runtime().signal("tick", || 0usize);
    tick.set(h.runtime(), 1);
    let stats = h.pump();

    assert_eq!(runs.get(), first, "the memoized scope re-ran");
    assert!(
        stats.nodes_copied > 0,
        "no node took the copy path (nodes_copied = 0, rebuilt = {}) — the \
         memo hit was re-lowered instead of copied",
        stats.nodes_rebuilt
    );
    // The button + its text live under the scope, so the span is >= 2 nodes.
    assert!(
        stats.nodes_copied >= 2,
        "only {} node(s) copied; the scope subtree is larger than that",
        stats.nodes_copied
    );
    h.assert_view_coherent();
}

/// Interaction state is host state, not retained work. A node inside a copied
/// span must reflect the *live* hover, not whatever it had when the span was
/// last lowered — in both directions.
#[test]
fn hover_state_inside_a_copied_span_tracks_the_live_pointer() {
    let (mut h, runs) = app_with_button_inside_a_scope();
    h.pump();
    let baseline = runs.get();
    assert_eq!(bg_of(&h.get_styles("#b")).as_deref(), Some("#00ff00ff"));

    // Hover the button, which lives inside the memoized span.
    let p = center(h.node_bounds_by_id("b").expect("button laid out"));
    h.inject(Event::PointerMove(PointerEvent::at(p)));
    h.pump();
    assert_eq!(
        bg_of(&h.get_styles("#b")).as_deref(),
        Some("#ff0000ff"),
        "hover did not apply to a node inside a memoized scope"
    );

    // Now force a rebuild *while hovered*: the span is copied, and the copy
    // must carry the hover with it.
    let tick: Signal<usize> = h.runtime().signal("tick", || 0usize);
    tick.set(h.runtime(), 1);
    let stats = h.pump();
    assert!(stats.nodes_copied > 0, "the span was not copied");
    assert_eq!(
        bg_of(&h.get_styles("#b")).as_deref(),
        Some("#ff0000ff"),
        "the copied span lost the live hover state"
    );

    // Unhover, then rebuild again: the copy must not resurrect a stale hover.
    h.inject(Event::PointerMove(PointerEvent::at(Point::new(2.0, 2.0))));
    h.pump();
    tick.set(h.runtime(), 2);
    let stats = h.pump();
    assert!(stats.nodes_copied > 0, "the span was not copied");
    assert_eq!(
        bg_of(&h.get_styles("#b")).as_deref(),
        Some("#00ff00ff"),
        "the copied span kept a hover the pointer had already left"
    );

    assert_eq!(runs.get(), baseline, "the scope re-ran during all of that");
    h.assert_view_coherent();
}

/// `copy_span` remaps the span records of scopes *nested* inside the span it
/// copies (`root_map`). If that remapping is wrong the inner scope's recorded
/// root points at a dead node, and the next build either misses the memo or
/// splices onto the wrong subtree.
#[test]
fn nested_scopes_are_remapped_when_the_outer_span_is_copied() {
    let outer_runs = Rc::new(Cell::new(0u32));
    let inner_runs = Rc::new(Cell::new(0u32));
    let (o, i) = (outer_runs.clone(), inner_runs.clone());
    let mut h = App::new(move |cx| {
        let tick: Signal<usize> = cx.signal("tick", || 0usize);
        let n = tick.get(cx.runtime());
        let (o, i) = (o.clone(), i.clone());
        col![
            widgets::text(format!("tick {n}")),
            cx.scope("outer", move |cx| {
                o.set(o.get() + 1);
                let i = i.clone();
                col![
                    widgets::text("outer content"),
                    cx.scope("inner", move |cx| {
                        i.set(i.get() + 1);
                        let rows: Signal<usize> = cx.runtime().signal("rows", || 1usize);
                        let count = rows.get(cx.runtime());
                        widgets::column(
                            (0..count)
                                .map(|k| widgets::text(format!("row {k}")))
                                .collect::<Vec<_>>(),
                        )
                    }),
                ]
            }),
        ]
    })
    .run_headless(Size::new(300.0, 300.0));
    h.pump();
    let (o0, i0) = (outer_runs.get(), inner_runs.get());
    let (_, inner_before) = h
        .scope_span(ScopePath::root().child("outer").child("inner"))
        .expect("nested span recorded");
    assert_eq!(inner_before, 2, "column + 1 row");

    // Rebuild with both scopes hitting their memo: the outer span is copied,
    // and the inner span record must be remapped onto the copied nodes.
    let tick: Signal<usize> = h.runtime().signal("tick", || 0usize);
    tick.set(h.runtime(), 1);
    let stats = h.pump();
    assert!(stats.nodes_copied > 0, "the outer span was not copied");
    assert_eq!(outer_runs.get(), o0, "the outer scope re-ran");
    assert_eq!(inner_runs.get(), i0, "the inner scope re-ran");

    let (_, inner_after) = h
        .scope_span(ScopePath::root().child("outer").child("inner"))
        .expect("nested span survives the copy");
    assert_eq!(inner_after, inner_before, "nested span count changed");
    // There is no public liveness accessor for a raw `NodeIndex`, and adding
    // one just for an assert would widen the API for no user. The remap is
    // proven behaviourally instead, below: if the record pointed at a dead
    // node, the inner scope could not re-run onto it.

    // The remap is only proven once the inner scope re-runs *through* it:
    // write the inner dep and check the subtree actually grows.
    let rows: Signal<usize> = h.runtime().signal("rows", || 1usize);
    rows.set(h.runtime(), 3);
    h.pump();
    assert!(inner_runs.get() > i0, "the inner scope never re-ran");
    let (_, grown) = h
        .scope_span(ScopePath::root().child("outer").child("inner"))
        .expect("nested span still recorded");
    assert_eq!(grown, 4, "column + 3 rows after the inner signal write");
    h.assert_view_coherent();
}
