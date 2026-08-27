//! WT-EXP P6 — `@keyframes` against the sink.
//!
//! The architectural question was settled by the transition prototype:
//! animation state must live in a registry keyed independently of the build.
//! What is new here is arithmetic and lifetime — multi-stop bracketing,
//! delay/looping/finite counts/alternation, and collecting timelines whose
//! nodes have gone.
//!
//! And one consequence transitions never had, because a transition always ends:
//! **an infinite timeline never finishes**, so a span containing one is refused
//! forever. `Spinner` and `Skeleton` animate continuously, so a loading screen
//! is precisely where memoization would quietly stop working. The last two
//! tests pin what that costs.

use lumen_core::semantics::Role;
use lumen_core::{Color, NodeIndex};
use lumen_layout::LayoutStyle;
use lumen_widgets::direct::{sample_timeline, KeyStop, StyleEnv, TreeSink, VisualState};

const ROWS: usize = 10;
const ANIMATED: usize = 4;

fn red() -> Color {
    Color::srgb8(0xff, 0x00, 0x00, 0xff)
}
fn green() -> Color {
    Color::srgb8(0x00, 0xff, 0x00, 0xff)
}
fn blue() -> Color {
    Color::srgb8(0x00, 0x00, 0xff, 0xff)
}

/// `red -> green -> blue` at 0 / 50 / 100 percent.
fn three_stops() -> Vec<(f32, KeyStop)> {
    vec![
        (
            0.0,
            KeyStop {
                background: Some(red()),
                ..KeyStop::default()
            },
        ),
        (
            0.5,
            KeyStop {
                background: Some(green()),
                ..KeyStop::default()
            },
        ),
        (
            1.0,
            KeyStop {
                background: Some(blue()),
                ..KeyStop::default()
            },
        ),
    ]
}

/// A sheet that plays `pulse` on `.anim`.
fn sheet(count: Option<f32>) -> String {
    let c = match count {
        None => "infinite".to_string(),
        Some(n) => format!("{n}"),
    };
    format!(".anim {{ animation: pulse 100ms linear 0ms {c}; }}")
}

fn sink(count: Option<f32>) -> TreeSink {
    let mut s = TreeSink::new().with_styles(
        StyleEnv::from_source(&sheet(count)).expect("parses"),
        VisualState::default(),
    );
    s.add_keyframes("pulse", three_stops());
    s
}

fn row(s: &mut TreeSink, p: Option<NodeIndex>, i: usize) -> (NodeIndex, lumen_layout::LayoutNode) {
    let mut d = s.node(p, Role::Group).id(format!("row{i}"));
    if i == ANIMATED {
        d = d.class("anim");
    }
    let node = d.resolve();
    let n = node.index();
    (n, node.end(&LayoutStyle::default(), &[], false))
}

fn frame(s: &mut TreeSink, now: f64) {
    s.set_clock(now);
    s.begin_frame();
    let mut root = s.node(None, Role::Group).resolve();
    let rn = root.index();
    let mut lns = Vec::with_capacity(ROWS);
    for i in 0..ROWS {
        let (_, ln) = root
            .sink()
            .scope(Some(rn), i as u64, 1, move |s, p| row(s, p, i));
        lns.push(ln);
    }
    root.end(&LayoutStyle::default(), &lns, false);
    s.end_frame();
    s.collect_animations();
    s.assert_balanced();
}

fn bg(s: &TreeSink, i: usize) -> Option<Color> {
    let want = format!("row{i}");
    s.tree
        .iter_live()
        .filter(|n| s.meta.contains(*n))
        .find(|n| s.meta.string_id(*n).map(|x| x.as_str()) == Some(want.as_str()))
        .and_then(|n| s.meta.background(n))
}

// --- the arithmetic --------------------------------------------------------

#[test]
fn sampling_lands_between_the_bracketing_pair() {
    // The off-by-one that would silently produce a plausible-but-wrong colour:
    // at 25% the value is between stop 0 and stop 1, never stop 0 and stop 2.
    let s = three_stops();
    let q = sample_timeline(&s, 0.25).background.unwrap();
    assert!(
        q.r > 0.4 && q.r < 0.6,
        "halfway from red toward green: {q:?}"
    );
    assert!(q.g > 0.4 && q.g < 0.6, "{q:?}");
    assert!(
        q.b < 0.01,
        "blue is the FAR stop and must not leak in: {q:?}"
    );

    let q = sample_timeline(&s, 0.75).background.unwrap();
    assert!(q.r < 0.01, "red is now the far stop: {q:?}");
    assert!(q.g > 0.4 && q.g < 0.6, "{q:?}");
    assert!(q.b > 0.4 && q.b < 0.6, "{q:?}");
}

#[test]
fn sampling_clamps_at_both_ends() {
    let s = three_stops();
    assert_eq!(sample_timeline(&s, 0.0).background, Some(red()));
    assert_eq!(sample_timeline(&s, 1.0).background, Some(blue()));
    assert_eq!(sample_timeline(&s, -5.0).background, Some(red()));
    assert_eq!(sample_timeline(&s, 5.0).background, Some(blue()));
}

#[test]
fn an_empty_timeline_is_harmless() {
    assert_eq!(sample_timeline(&Vec::new(), 0.5), KeyStop::default());
}

// --- scheduling ------------------------------------------------------------

#[test]
fn a_timeline_advances_across_frames() {
    let mut s = sink(None);
    let mut seen = Vec::new();
    for t in [0.0, 25.0, 50.0, 75.0] {
        frame(&mut s, t);
        seen.push(bg(&s, ANIMATED).expect("styled"));
    }
    assert!(seen[0].r > 0.9, "starts red: {:?}", seen[0]);
    assert!(seen[2].g > 0.9, "green at the midpoint: {:?}", seen[2]);
    assert!(seen[3].b > 0.4, "heading toward blue by 75%: {:?}", seen[3]);
}

#[test]
fn an_unanimated_sibling_is_untouched() {
    let mut s = sink(None);
    frame(&mut s, 0.0);
    frame(&mut s, 50.0);
    assert!(bg(&s, 0).is_none(), "a row without `.anim` gets no fill");
}

#[test]
fn a_finite_timeline_latches_on_its_last_stop() {
    let mut s = sink(Some(1.0));
    frame(&mut s, 0.0);
    frame(&mut s, 500.0); // long past one 100 ms iteration
    let c = bg(&s, ANIMATED).expect("styled");
    assert!(
        c.b > 0.9,
        "it holds the final stop rather than snapping back to red: {c:?}"
    );
    assert!(
        !s.keyframes_running(),
        "and it is no longer running, so it stops refusing its span"
    );
}

// --- the coupling to memoization -------------------------------------------

#[test]
fn a_memoized_timeline_does_not_freeze() {
    // Same failure mode as transitions: every scope's dep is unchanged, so
    // without the refusal the animated row splices and never moves again.
    let mut s = sink(None);
    frame(&mut s, 0.0);
    frame(&mut s, 10.0);
    let early = bg(&s, ANIMATED).unwrap();
    frame(&mut s, 45.0);
    let late = bg(&s, ANIMATED).unwrap();
    assert!(
        late.g > early.g + 0.3,
        "the timeline kept playing across memoized frames ({early:?} -> {late:?})"
    );
}

#[test]
fn only_the_animated_span_is_refused() {
    let mut s = sink(None);
    frame(&mut s, 0.0);
    frame(&mut s, 20.0);
    let st = s.stats();
    assert_eq!(st.rebuilt, 1, "only the animated scope re-ran: {st:?}");
    assert_eq!(st.spliced, ROWS - 1, "the rest still spliced: {st:?}");
}

#[test]
fn splicing_resumes_after_a_finite_timeline_ends() {
    let mut s = sink(Some(1.0));
    frame(&mut s, 0.0);
    frame(&mut s, 50.0);
    assert_eq!(s.stats().rebuilt, 1, "refused while running");
    frame(&mut s, 500.0); // ends here and latches
    frame(&mut s, 520.0);
    let st = s.stats();
    assert_eq!(
        st.spliced, ROWS,
        "everything splices again once nothing is running: {st:?}"
    );
}

#[test]
fn an_infinite_timeline_refuses_its_span_forever() {
    // The consequence transitions never had. This is not a bug to fix — it is
    // the honest cost, and it is pinned here so it cannot regress silently into
    // a frozen spinner instead.
    let mut s = sink(None);
    frame(&mut s, 0.0);
    for f in 1..40 {
        frame(&mut s, f as f64 * 16.0);
    }
    let st = s.stats();
    assert_eq!(
        st.rebuilt, 1,
        "forty frames later the animated scope is STILL rebuilding every frame"
    );
    assert_eq!(
        st.spliced,
        ROWS - 1,
        "its siblings are unaffected, which is what keeps the cost bounded"
    );
    assert!(s.keyframes_running(), "and it never finishes");
}

// --- lifetime --------------------------------------------------------------

#[test]
fn a_vanished_nodes_timeline_is_collected() {
    // Without collection an app that churns animated nodes leaks a registry
    // entry per node forever — and every one keeps refusing splices.
    let mut s = sink(None);
    frame(&mut s, 0.0);
    frame(&mut s, 16.0);
    assert!(s.keyframes_running(), "one timeline is live");

    // A frame with no rows at all: the animated node is gone.
    s.set_clock(32.0);
    s.begin_frame();
    let root = s.node(None, Role::Group).resolve();
    root.end(&LayoutStyle::default(), &[], false);
    s.end_frame();
    s.collect_animations();
    s.assert_balanced();

    assert!(
        !s.keyframes_running(),
        "the timeline was collected with its node; leaving it would refuse a \
         span that no longer exists and never expire"
    );
}

#[test]
fn reduced_motion_suppresses_the_timeline() {
    let mut s = sink(None);
    s.set_reduced_motion(true);
    frame(&mut s, 0.0);
    frame(&mut s, 50.0);
    assert!(
        bg(&s, ANIMATED).is_none(),
        "no animation ran, so the node keeps the styling it would otherwise have"
    );
    assert!(!s.keyframes_running());
}
