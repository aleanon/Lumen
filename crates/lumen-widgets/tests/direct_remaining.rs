//! WT-EXP P7–P9 — the last three unknowns.
//!
//! **P7 container queries.** `@media container(...)` tests the nearest
//! `.container()` ancestor's size, taken from the *previous* layout because
//! this node has not been laid out yet. It feeds the context hash, so a
//! container that resized makes its descendants resolve differently with no
//! change to their own data — the P2 hazard in another guise.
//!
//! **P8 code hot reload.** A tier-2 swap replaces a component's `build()` in
//! place. Every span that component produced was made by code that no longer
//! exists, so all of them are stale — the same shape as a stylesheet edit.
//!
//! **P9 snapshot / restore.** `AppSnapshot` is `{ state, focused }` and never
//! mentions `Element`, so direct lowering cannot affect it — *except* that
//! focus is keyed by `StableId` while a structured `NodeId` renders its string
//! on demand. Whether those two identity forms interoperate is the one thing
//! actually worth testing here.

use lumen_core::semantics::Role;
use lumen_core::{Color, NodeIndex};
use lumen_layout::{Dim, LayoutStyle};
use lumen_widgets::direct::{StyleEnv, TreeSink, VisualState};

fn styled(src: &str) -> TreeSink {
    TreeSink::new().with_styles(
        StyleEnv::from_source(src).expect("parses"),
        VisualState::default(),
    )
}

fn body(s: &mut TreeSink, p: Option<NodeIndex>) -> (NodeIndex, lumen_layout::LayoutNode) {
    let node = s.node(p, Role::Button).label("Go").resolve();
    let n = node.index();
    (n, node.end(&LayoutStyle::default(), &[], false))
}

fn button_width(s: &TreeSink) -> Option<Dim> {
    s.tree
        .iter_live()
        .filter(|n| s.meta.contains(*n))
        .find(|n| s.meta.role(*n) == Role::Button)
        .map(|n| s.meta.layout_style(n).width)
}

// --- P7: container queries -------------------------------------------------

/// One frame with a container whose size comes from `prev`.
fn container_frame(s: &mut TreeSink, prev: Option<(f64, f64)>) {
    if let Some(size) = prev {
        s.record_container_sizes(vec![size]);
    }
    s.begin_frame();
    let mut root = s.node(None, Role::Group).resolve();
    // The guard's borrow rules force the container's children to be written
    // through the container's own handle, which is exactly the ordering the
    // container stack needs — one more mistake the types make unrepresentable.
    let hl = {
        let mut holder = root.begin_child(Role::Group).resolve();
        let hn = holder.index();
        holder.sink().enter_container(hn, 0);
        // The scope's own dep never changes; only the container's size does.
        let (_, inner) = holder.sink().scope(Some(hn), 1, 1, body);
        holder.sink().exit_container();
        holder.end(&LayoutStyle::default(), &[inner], false)
    };
    root.end(&LayoutStyle::default(), &[hl], false);
    s.end_frame();
    s.assert_balanced();
}

#[test]
fn a_container_query_matches_on_the_containers_size() {
    let mut s = styled("@media container(width > 500px) { button { width: 400px; } }");
    // First frame: no previous layout, so the query fails closed.
    container_frame(&mut s, None);
    assert_eq!(
        button_width(&s),
        Some(Dim::Auto),
        "with no laid-out size yet the query fails closed, which is the correct \
         answer rather than a missing feature"
    );

    // Now the container has been measured wide.
    container_frame(&mut s, Some((800.0, 100.0)));
    assert_eq!(
        button_width(&s),
        Some(Dim::px(400.0)),
        "the query matched against the container, not the window"
    );
}

#[test]
fn a_resized_container_invalidates_its_descendants_spans() {
    // The P2 hazard in another guise: the scope's data is identical, but the
    // container it sits in changed size, so its rules resolve differently.
    let mut s = styled("@media container(width > 500px) { button { width: 400px; } }");
    container_frame(&mut s, Some((800.0, 100.0)));
    assert_eq!(button_width(&s), Some(Dim::px(400.0)), "wide: rule matches");

    container_frame(&mut s, Some((300.0, 100.0)));
    assert_eq!(
        button_width(&s),
        Some(Dim::Auto),
        "narrow: the rule no longer matches. If this is still 400px the span \
         was spliced across a container resize and the query is stale"
    );
    assert_eq!(s.stats().rebuilt, 1, "the resize re-ran the scope");
}

#[test]
fn an_unchanged_container_still_splices() {
    // The guard must not be so blunt that any container defeats memoization.
    let mut s = styled("@media container(width > 500px) { button { width: 400px; } }");
    container_frame(&mut s, Some((800.0, 100.0)));
    container_frame(&mut s, Some((800.0, 100.0)));
    assert_eq!(s.stats().spliced, 1, "same size, same dep: reused");
    assert_eq!(s.stats().rebuilt, 0);
}

// --- P8: code hot reload ---------------------------------------------------

fn plain_frame(s: &mut TreeSink) {
    s.begin_frame();
    let mut root = s.node(None, Role::Group).resolve();
    let rn = root.index();
    let (_, ln) = root.sink().scope(Some(rn), 1, 1, body);
    root.end(&LayoutStyle::default(), &[ln], false);
    s.end_frame();
    s.assert_balanced();
}

#[test]
fn a_code_swap_invalidates_every_span() {
    // A tier-2 swap replaced the component's build(). Its output is retained in
    // the tree, and nothing about the scope's *data* changed — so without a
    // build generation the app would keep showing the old component's nodes.
    let mut s = TreeSink::new();
    plain_frame(&mut s);
    plain_frame(&mut s);
    assert_eq!(s.stats().spliced, 1, "steady state is memoized");

    s.set_build_generation(1); // the dylib was swapped
    plain_frame(&mut s);
    assert_eq!(
        s.stats().rebuilt,
        1,
        "the swap re-ran the scope; a spliced span would still be the old \
         component's output"
    );

    plain_frame(&mut s);
    assert_eq!(
        s.stats().spliced,
        1,
        "and the next frame is memoized again, so a swap costs one frame"
    );
}

#[test]
fn an_unchanged_build_generation_still_splices() {
    let mut s = TreeSink::new();
    plain_frame(&mut s);
    s.set_build_generation(0); // a rebuild that produced identical code
    plain_frame(&mut s);
    assert_eq!(s.stats().spliced, 1, "no swap, no invalidation");
}

// --- P9: snapshot / restore ------------------------------------------------

#[test]
fn focus_matches_a_structured_id_by_its_rendered_string() {
    // `AppSnapshot` stores `focused: Option<StableId>`, and restore re-applies
    // it directly. A node built with `id_at("row", 5)` never mints "row5" — so
    // if the two forms did not agree, restoring focus would silently focus
    // nothing and `:focus` styling would vanish after every reload.
    let mut s = TreeSink::new().with_styles(
        StyleEnv::from_source("button:focus { background: #00ff00; }").expect("parses"),
        VisualState {
            focused: Some("row5".into()),
            hovered: None,
        },
    );
    let n = {
        let d = s.node(None, Role::Button).id_at("row", 5).resolve();
        let i = d.index();
        d.end(&LayoutStyle::default(), &[], false);
        i
    };
    let bg = s.meta.background(n).expect(
        "the `:focus` rule matched a structured id against the restored \
         StableId — if this is None the two identity forms do not interoperate \
         and focus restore is broken",
    );
    assert!(bg.g > 0.9, "focused: {bg:?}");
}

#[test]
fn an_unfocused_structured_id_does_not_match() {
    let mut s = TreeSink::new().with_styles(
        StyleEnv::from_source("button:focus { background: #00ff00; }").expect("parses"),
        VisualState {
            focused: Some("row9".into()),
            hovered: None,
        },
    );
    let n = {
        let d = s.node(None, Role::Button).id_at("row", 5).resolve();
        let i = d.index();
        d.end(&LayoutStyle::default(), &[], false);
        i
    };
    assert!(
        s.meta.background(n).is_none(),
        "row5 is not row9 — the index is part of the identity, so focus does \
         not smear across siblings"
    );
}

#[test]
fn a_structured_id_is_addressable_the_way_a_test_or_the_agent_asks() {
    // Everything that looks the UI up by id — selectors, `lumen-test`, the
    // agent protocol — goes through the string form. It has to be produced on
    // demand and be exactly what a string id would have been.
    let mut s = TreeSink::new();
    let n = {
        let d = s.node(None, Role::Button).id_at("row", 5).resolve();
        let i = d.index();
        d.end(&LayoutStyle::default(), &[], false);
        i
    };
    assert_eq!(
        s.meta.id_string(n, &s.symbols).as_deref(),
        Some("row5"),
        "the rendered form is what an agent query would match"
    );
    let _ = Color::WHITE;
}
