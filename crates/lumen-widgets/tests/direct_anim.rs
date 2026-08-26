//! WT-EXP P3 — transitions, and their coupling to memoization.
//!
//! `apply_transitions` blends a mid-flight transition into the resolved style,
//! and `splice_span` refuses any span containing an animating node because its
//! styles are mid-interpolation. Get that refusal wrong and the node **freezes**
//! at the frame it was first spliced — a silent bug with no panic, no
//! diagnostic, and no wrong value anywhere a test would normally look.
//!
//! So the freeze is demonstrated first. `a_memoized_transition_would_freeze_
//! without_the_refusal` fails if the refusal is removed.

use lumen_core::semantics::Role;
use lumen_core::{Color, NodeIndex};
use lumen_layout::LayoutStyle;
use lumen_widgets::direct::{Anim, TreeSink};

const ROWS: usize = 8;
/// The row carrying the transition.
const ANIMATED: usize = 3;

fn row(s: &mut TreeSink, p: Option<NodeIndex>, i: usize) -> (NodeIndex, lumen_layout::LayoutNode) {
    let node = s
        .node(p, Role::Group)
        .id(format!("row{i}"))
        .background(Color::srgb8(0x00, 0x00, 0x00, 0xff))
        .resolve();
    let n = node.index();
    (n, node.end(&LayoutStyle::default(), &[], false))
}

/// One frame at `now`, every row memoized on an unchanging dep.
fn frame(s: &mut TreeSink, now: f64) {
    s.set_clock(now);
    s.begin_frame();
    let mut root = s.node(None, Role::Group).resolve();
    let rn = root.index();
    let mut lns = Vec::with_capacity(ROWS);
    for i in 0..ROWS {
        let (_, ln) = root.sink().scope(Some(rn), i as u64, 1, move |s, p| row(s, p, i));
        lns.push(ln);
    }
    root.end(&LayoutStyle::default(), &lns, false);
    s.end_frame();
    s.assert_balanced();
}

fn background_of(s: &TreeSink, id: &str) -> Option<Color> {
    s.tree
        .subtree_preorder(s.tree.root())
        .into_iter()
        .filter(|n| s.tree.is_alive(*n))
        .filter(|n| s.meta.contains(*n))
        .find_map(|n| {
            let is_it = s.meta.string_id(n).map(|i| i.as_str()) == Some(id);
            is_it.then(|| s.meta.background(n))
        })
        .flatten()
}

#[test]
fn a_transition_blends_across_frames() {
    let mut s = TreeSink::new();
    frame(&mut s, 0.0);
    s.start_transition(
        format!("row{ANIMATED}"),
        Anim {
            from: Color::srgb8(0x00, 0x00, 0x00, 0xff),
            to: Color::srgb8(0xff, 0xff, 0xff, 0xff),
            start_ms: 0.0,
            dur_ms: 100.0,
        },
    );

    let mut seen = Vec::new();
    for t in [0.0, 25.0, 50.0, 75.0, 100.0] {
        frame(&mut s, t);
        seen.push(background_of(&s, &format!("row{ANIMATED}")).expect("row exists").r);
    }
    for w in seen.windows(2) {
        assert!(
            w[1] >= w[0],
            "the blend advances monotonically: {seen:?}"
        );
    }
    assert!(seen[0] < 0.1, "starts at the `from` colour: {seen:?}");
    assert!(seen[4] > 0.9, "reaches the `to` colour: {seen:?}");
}

#[test]
fn a_memoized_transition_would_freeze_without_the_refusal() {
    // The bug this guards. Every row is memoized on an unchanging dep, so
    // without AN1 the animated row splices on frame 2 and its colour never
    // moves again.
    let mut s = TreeSink::new();
    frame(&mut s, 0.0);
    s.start_transition(
        format!("row{ANIMATED}"),
        Anim {
            from: Color::srgb8(0x00, 0x00, 0x00, 0xff),
            to: Color::srgb8(0xff, 0xff, 0xff, 0xff),
            start_ms: 0.0,
            dur_ms: 100.0,
        },
    );
    frame(&mut s, 10.0);
    let early = background_of(&s, &format!("row{ANIMATED}")).unwrap().r;
    frame(&mut s, 90.0);
    let late = background_of(&s, &format!("row{ANIMATED}")).unwrap().r;
    assert!(
        late > early + 0.5,
        "the transition kept running across memoized frames ({early} -> {late}); \
         if these are equal the span was spliced while mid-interpolation and \
         the node froze"
    );
}

#[test]
fn only_the_animating_span_is_refused() {
    // The refusal must be surgical: an animation in one scope must not stop
    // its siblings from splicing, or one hover would cost a full rebuild.
    let mut s = TreeSink::new();
    frame(&mut s, 0.0);
    s.start_transition(
        format!("row{ANIMATED}"),
        Anim {
            from: Color::srgb8(0x00, 0x00, 0x00, 0xff),
            to: Color::srgb8(0xff, 0xff, 0xff, 0xff),
            start_ms: 0.0,
            dur_ms: 100.0,
        },
    );
    frame(&mut s, 10.0);
    frame(&mut s, 20.0);
    let st = s.stats();
    assert_eq!(st.rebuilt, 1, "only the animating scope re-ran: {st:?}");
    assert_eq!(st.spliced, ROWS - 1, "every other scope still spliced: {st:?}");
}

#[test]
fn splicing_resumes_once_the_transition_finishes() {
    // And it must be temporary: once the blend completes, the node goes back to
    // being memoizable, or an app that ever animated stays expensive forever.
    let mut s = TreeSink::new();
    frame(&mut s, 0.0);
    s.start_transition(
        format!("row{ANIMATED}"),
        Anim {
            from: Color::srgb8(0x00, 0x00, 0x00, 0xff),
            to: Color::srgb8(0xff, 0xff, 0xff, 0xff),
            start_ms: 0.0,
            dur_ms: 100.0,
        },
    );
    frame(&mut s, 50.0);
    assert!(s.animating(), "mid-flight");
    frame(&mut s, 200.0); // past the end: the blend completes and deregisters
    assert!(!s.animating(), "the transition finished and was dropped");
    frame(&mut s, 210.0);
    let st = s.stats();
    assert_eq!(
        st.spliced, ROWS,
        "every scope splices again once nothing is animating: {st:?}"
    );
    assert_eq!(st.rebuilt, 0);
}

#[test]
fn a_frame_with_no_animation_pays_nothing() {
    // The engine gates the span scan on an animation actually running. Without
    // that gate every memo hit would walk its subtree.
    let mut s = TreeSink::new();
    frame(&mut s, 0.0);
    frame(&mut s, 16.0);
    assert!(!s.animating(), "nothing is running");
    assert_eq!(s.stats().spliced, ROWS, "and everything spliced");
}
