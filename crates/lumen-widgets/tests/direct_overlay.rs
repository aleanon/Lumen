//! WT-EXP P2 — overlay/z routing, and the memo context it feeds.
//!
//! The engine guards a splice with `span_ctx_hash`: the ancestor chain, the
//! container size, the overlay flag, and the hidden/disabled depths. A retained
//! span may only be reused if that whole *outside context* is unchanged, because
//! all of it feeds the cascade.
//!
//! The prototype's `scope()` checked only the caller's `dep`. This file
//! demonstrates what that costs before fixing it: same data, different
//! surroundings, wrongly reused.

use lumen_core::semantics::Role;
use lumen_core::NodeIndex;
use lumen_layout::LayoutStyle;
use lumen_widgets::direct::{StyleEnv, TreeSink, VisualState};

fn styled(src: &str) -> TreeSink {
    TreeSink::new().with_styles(
        StyleEnv::from_source(src).expect("parses"),
        VisualState::default(),
    )
}

/// The memoized subtree under test: a single button.
fn body(s: &mut TreeSink, p: Option<NodeIndex>) -> (NodeIndex, lumen_layout::LayoutNode) {
    let node = s.node(p, Role::Button).label("Go").resolve();
    let n = node.index();
    (n, node.end(&LayoutStyle::default(), &[], false))
}

/// Build a frame where the memoized scope sits under `wrapper_class`.
fn frame(s: &mut TreeSink, wrapper_class: &str) {
    s.begin_frame();
    let mut root = s
        .node(None, Role::Group)
        .class(wrapper_class.to_string())
        .resolve();
    let rn = root.index();
    // The scope's own dependency is UNCHANGED between frames; only the
    // surrounding class differs.
    let (_, ln) = root.sink().scope(Some(rn), 1, 1, body);
    root.end(&LayoutStyle::default(), &[ln], false);
    s.end_frame();
    s.assert_balanced();
}

fn button_background(s: &TreeSink) -> Option<lumen_core::Color> {
    s.tree
        .subtree_preorder(s.tree.root())
        .into_iter()
        .filter(|n| s.tree.is_alive(*n))
        .find_map(|n| {
            let m = s.meta.get(&n)?;
            (m.role == Role::Button).then_some(m.background)
        })
        .flatten()
}

#[test]
fn a_scope_under_a_changed_ancestor_is_not_wrongly_reused() {
    // `.danger button` matches only under the second wrapper. The scope's own
    // data never changes, so a memo keyed on `dep` alone would splice — and the
    // button would keep the styling it got under the *first* wrapper.
    let mut s = styled(".danger button { background: #ff0000; }");

    frame(&mut s, "calm");
    assert_eq!(
        button_background(&s),
        None,
        "under `.calm` the rule does not match"
    );

    frame(&mut s, "danger");
    let bg = button_background(&s).expect(
        "under `.danger` the rule matches — if this is None the span was \
         spliced despite its surrounding context changing, which is exactly \
         the bug `span_ctx_hash` exists to prevent",
    );
    assert!(bg.r > 0.9 && bg.g < 0.1, "the button is red: {bg:?}");
    assert_eq!(s.stats().rebuilt, 1, "the scope re-ran because its context changed");
}

#[test]
fn a_scope_in_an_unchanged_context_still_splices() {
    // The guard must not be so blunt that it defeats memoization.
    let mut s = styled(".danger button { background: #ff0000; }");
    frame(&mut s, "calm");
    frame(&mut s, "calm");
    assert_eq!(
        s.stats().spliced,
        1,
        "identical context and dep: the span was reused"
    );
    assert_eq!(s.stats().rebuilt, 0);
}

#[test]
fn an_overlay_subtree_gets_overlay_z() {
    let mut s = TreeSink::new();
    s.begin_frame();
    let mut root = s.node(None, Role::Group).resolve();
    let rn = root.index();
    // A normal child.
    let normal = root.begin_child(Role::Group).resolve();
    let nn = normal.index();
    let a = normal.end(&LayoutStyle::default(), &[], false);

    // An overlay child: it and its descendants route to the overlay pass.
    root.sink().enter_overlay();
    let over = root.begin_child(Role::Dialog).resolve();
    let on = over.index();
    let inner = {
        let mut o = over;
        let c = o.begin_child(Role::Button).resolve();
        let cn = c.index();
        let cl = c.end(&LayoutStyle::default(), &[], false);
        let ol = o.end(&LayoutStyle::default(), &[cl], false);
        (cn, ol)
    };
    root.sink().exit_overlay();
    root.end(&LayoutStyle::default(), &[a, inner.1], false);
    s.end_frame();
    s.assert_balanced();

    assert_eq!(s.tree.z(rn), 0, "the root is not an overlay");
    assert_eq!(s.tree.z(nn), 0, "a normal child is not an overlay");
    assert_eq!(
        s.tree.z(on),
        lumen_widgets::direct::OVERLAY_Z,
        "the overlay root routes above the page"
    );
    assert_eq!(
        s.tree.z(inner.0),
        lumen_widgets::direct::OVERLAY_Z,
        "and so does everything inside it — overlay is inherited, not local"
    );
}

#[test]
fn a_scope_moved_into_an_overlay_is_not_wrongly_reused() {
    // Overlay membership changes paint order and feeds the context hash, so a
    // span retained outside an overlay must not be spliced into one.
    let mut s = TreeSink::new();

    let build = |s: &mut TreeSink, overlay: bool| {
        s.begin_frame();
        let mut root = s.node(None, Role::Group).resolve();
        let rn = root.index();
        if overlay {
            root.sink().enter_overlay();
        }
        let (_, ln) = root.sink().scope(Some(rn), 7, 1, body);
        if overlay {
            root.sink().exit_overlay();
        }
        root.end(&LayoutStyle::default(), &[ln], false);
        s.end_frame();
        s.assert_balanced();
    };

    build(&mut s, false);
    let flat = s
        .tree
        .subtree_preorder(s.tree.root())
        .into_iter()
        .find(|n| s.meta.get(n).map(|m| m.role) == Some(Role::Button))
        .expect("button exists");
    assert_eq!(s.tree.z(flat), 0);

    build(&mut s, true);
    assert_eq!(s.stats().rebuilt, 1, "moving into an overlay re-ran the scope");
    let inside = s
        .tree
        .subtree_preorder(s.tree.root())
        .into_iter()
        .find(|n| s.meta.get(n).map(|m| m.role) == Some(Role::Button))
        .expect("button exists");
    assert_eq!(
        s.tree.z(inside),
        lumen_widgets::direct::OVERLAY_Z,
        "the button now paints in the overlay pass; a spliced span would have \
         kept z=0 and painted under the page"
    );
}
