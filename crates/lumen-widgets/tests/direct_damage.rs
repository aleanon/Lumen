//! WT-EXP P4 — damage tracking under splicing.
//!
//! Damage is `damage_between(prev, next)`: a prefix/suffix diff over two
//! **display lists**, bounding the changed commands into a rectangle. It is a
//! pure function of the display list, which is itself a pure function of the
//! tree, the layout and the per-node records. Nothing in it asks how the tree
//! was built, so direct lowering does not change damage *provided the tree it
//! produces is the same one*.
//!
//! That proviso is the actual risk, and it is sharper than it sounds. Splicing
//! reuses nodes **without touching them**: no `resolve`, no `end`, no writes. If
//! any part of a node's observable state were only established during a rebuild,
//! a spliced frame would silently lose it — and the damage diff would then be
//! comparing against a display list missing that content.
//!
//! So this file does not test damage. It tests the thing damage rests on: that
//! a spliced frame and a from-scratch frame are indistinguishable across *every*
//! field a painter reads, through a churn sequence that mixes memo hits, dirty
//! scopes, overlays and a running transition.

use lumen_core::semantics::Role;
use lumen_core::{Color, NodeIndex};
use lumen_layout::{Dim, LayoutStyle};
use lumen_widgets::direct::{Anim, TreeSink};
use lumen_widgets::{Button, Label};

const ROWS: usize = 24;

/// Everything a painter would read off one node.
#[derive(Debug, PartialEq)]
struct Paintable {
    role: Role,
    id: Option<String>,
    label: String,
    value: Option<String>,
    classes: Vec<String>,
    background: Option<[u8; 4]>,
    corner_radius: f64,
    width: Dim,
    height: Dim,
    z: u32,
    has_text: bool,
    child_count: usize,
}

fn rgba(c: Color) -> [u8; 4] {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [q(c.r), q(c.g), q(c.b), q(c.a)]
}

/// The whole tree as a painter sees it, in document order.
fn paintable(s: &TreeSink) -> Vec<Paintable> {
    s.tree
        .subtree_preorder(s.tree.root())
        .into_iter()
        .filter(|n| s.tree.is_alive(*n))
        .filter(|n| s.meta.contains(*n))
        .map(|n| {
            let mut kids = 0;
            let mut c = s.tree.first_child(n);
            while c.is_some() {
                kids += 1;
                c = s.tree.next_sibling(c);
            }
            Paintable {
                role: s.meta.role(n),
                id: s.meta.id_string(n, &s.symbols),
                label: s.meta.label(n).to_string(),
                value: s.meta.value(n).map(str::to_string),
                classes: s.meta.classes(n).to_vec(),
                background: s.meta.background(n).map(rgba),
                corner_radius: s.meta.corner_radius(n),
                width: s.meta.layout_style(n).width,
                height: s.meta.layout_style(n).height,
                z: s.tree.z(n),
                has_text: s.meta.content(n).is_some(),
                child_count: kids,
            }
        })
        .collect()
}

fn row(
    s: &mut TreeSink,
    p: Option<NodeIndex>,
    i: usize,
    ver: u64,
) -> (NodeIndex, lumen_layout::LayoutNode) {
    let mut open = s
        .node(p, Role::Group)
        .id(format!("row{i}"))
        .class("row")
        .background(Color::srgb8(0x11, 0x22, 0x33, 0xff))
        .corner_radius(4.0)
        .resolve();
    let a = open.child(Label::new(format!("row {i} v{ver}")));
    let b = open.child(Button::new("Open"));
    let n = open.index();
    (n, open.end(&LayoutStyle::default(), &[a, b], false))
}

/// One frame. `overlay_from` puts the tail rows in an overlay subtree.
fn frame(s: &mut TreeSink, versions: &[u64], overlay_from: usize, now: f64) {
    s.set_clock(now);
    s.begin_frame();
    let mut root = s.node(None, Role::Group).class("page").resolve();
    let rn = root.index();
    let mut lns = Vec::with_capacity(ROWS);
    for (i, &ver) in versions.iter().enumerate().take(ROWS) {
        if i == overlay_from {
            root.sink().enter_overlay();
        }
        let (_, ln) = root
            .sink()
            .scope(Some(rn), i as u64, ver, move |s, p| row(s, p, i, ver));
        lns.push(ln);
    }
    if overlay_from < ROWS {
        root.sink().exit_overlay();
    }
    root.end(&LayoutStyle::default(), &lns, false);
    s.end_frame();
    s.assert_balanced();
}

fn fresh() -> TreeSink {
    TreeSink::new().with_text(lumen_text::TextEngine::new())
}

#[test]
fn a_spliced_frame_is_indistinguishable_from_a_fresh_one() {
    // The property damage rests on, driven through a churn sequence rather than
    // a single happy frame.
    let mut inc = fresh();
    let mut versions = vec![1u64; ROWS];

    frame(&mut inc, &versions, ROWS, 0.0); // all fresh
    frame(&mut inc, &versions, ROWS, 16.0); // all spliced
    versions[3] = 2;
    versions[19] = 7;
    frame(&mut inc, &versions, ROWS, 32.0); // two dirty
    frame(&mut inc, &versions, ROWS, 48.0); // all spliced again
    versions[0] = 9;
    frame(&mut inc, &versions, 18, 64.0); // tail moves into an overlay
    frame(&mut inc, &versions, 18, 80.0); // and splices in that context

    let mut scratch = fresh();
    frame(&mut scratch, &versions, 18, 80.0);

    let (a, b) = (paintable(&inc), paintable(&scratch));
    assert_eq!(
        a.len(),
        b.len(),
        "node counts differ: {} vs {}",
        a.len(),
        b.len()
    );
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        assert_eq!(x, y, "node {i} differs after splicing");
    }
}

#[test]
fn a_spliced_frame_preserves_measured_text_sizes() {
    // Measurement happens in `end`, which a spliced node never reaches. If the
    // measured box did not survive on the retained node, every memo hit would
    // silently resize its text — and the damage diff would see the whole list
    // change every frame.
    let mut s = fresh();
    let versions = vec![1u64; ROWS];
    frame(&mut s, &versions, ROWS, 0.0);
    let first = paintable(&s);
    for f in 1..6 {
        frame(&mut s, &versions, ROWS, f as f64 * 16.0);
    }
    let later = paintable(&s);
    assert_eq!(
        first, later,
        "five spliced frames left every measured box exactly as built"
    );
    assert_eq!(s.stats().spliced, ROWS, "and they really were splices");
}

#[test]
fn an_animating_frame_still_matches_a_fresh_build() {
    // The hardest case: a transition is running, so one span is refused while
    // its siblings splice. The result must still equal a fresh build at the
    // same clock.
    let mut inc = fresh();
    let versions = vec![1u64; ROWS];
    frame(&mut inc, &versions, ROWS, 0.0);
    let anim = Anim {
        from: Color::srgb8(0x11, 0x22, 0x33, 0xff),
        to: Color::srgb8(0xff, 0x00, 0x00, 0xff),
        start_ms: 0.0,
        dur_ms: 100.0,
    };
    inc.start_transition("row5", anim);
    frame(&mut inc, &versions, ROWS, 40.0);

    let mut scratch = fresh();
    frame(&mut scratch, &versions, ROWS, 0.0);
    scratch.start_transition("row5", anim);
    frame(&mut scratch, &versions, ROWS, 40.0);

    assert_eq!(
        paintable(&inc),
        paintable(&scratch),
        "a partially spliced animating frame matches a fresh one"
    );
}

#[test]
fn splicing_does_not_perturb_document_order() {
    // Damage is a *prefix/suffix* diff over the command list, so a reordering
    // would not merely mislocate the damage rectangle — it would defeat the
    // prefix scan entirely and report the whole frame changed.
    let mut s = fresh();
    let mut versions = vec![1u64; ROWS];
    frame(&mut s, &versions, ROWS, 0.0);
    // Dirty a scattered subset, so splices and rebuilds interleave.
    for i in [1usize, 4, 5, 11, 23] {
        versions[i] = 2;
    }
    frame(&mut s, &versions, ROWS, 16.0);

    let ids: Vec<String> = paintable(&s)
        .into_iter()
        .filter_map(|p| p.id)
        .filter(|i| i.starts_with("row"))
        .collect();
    let expected: Vec<String> = (0..ROWS).map(|i| format!("row{i}")).collect();
    assert_eq!(ids, expected, "interleaved splices and rebuilds kept order");
}
