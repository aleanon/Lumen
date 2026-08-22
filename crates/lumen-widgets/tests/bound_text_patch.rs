//! F3.5: a text binding whose new value measures the same size patches in
//! place — no rebuild, no relayout, no view closure re-run.
//!
//! Text used to be classified structural on the grounds that a new string can
//! measure to a new size. That is a property of the *value*, not of the
//! binding: a label in a sized container, a wrapping paragraph that still
//! fills the same lines, or a virtual-list row with a fixed item height all
//! keep their box. `probe_tiers` prices the two paths on the same 3000-row
//! list: ~60 µs to patch against ~940 µs to rebuild.
//!
//! What must hold, and what these tests pin:
//!   * a same-size change patches, and the view still matches a fresh rebuild;
//!   * a size-CHANGING change still rebuilds, and shows the new text;
//!   * the accessible label follows the content (a patch that updated only one
//!     would drift from what a rebuild produces);
//!   * an author-fixed axis is never a reason to rebuild.

use std::cell::Cell;
use std::rc::Rc;

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_core::Dynamic;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, BuildCx};

/// Builds a label bound to signal `n`, rendered through `f`. Returns the
/// harness and a counter of how many times the view closure ran — the
/// observable that separates a patch from a rebuild.
fn app_with_bound_label(
    f: impl Fn(i64) -> String + Clone + 'static,
    width: Option<f32>,
) -> (lumen_widgets::Headless, Rc<Cell<u32>>) {
    let runs = Rc::new(Cell::new(0u32));
    let r = runs.clone();
    let h = App::new(move |cx: &mut BuildCx| {
        r.set(r.get() + 1);
        let n: Signal<i64> = cx.signal("n", || 0);
        let f = f.clone();
        let mut lbl = widgets::text("placeholder")
            .id("lbl")
            .bind_text(Dynamic::new(move |rt| f(n.get(rt))));
        if let Some(w) = width {
            lbl.style.width = Dim::px(w);
        }
        widgets::column(vec![lbl])
    })
    .run_headless(Size::new(300.0, 120.0));
    (h, runs)
}

fn label_of(h: &lumen_widgets::Headless) -> String {
    h.semantics_json().to_string()
}

#[test]
fn a_same_size_text_change_patches_without_rebuilding() {
    // Same glyph count, same glyphs, so the block measures identically.
    let (mut h, runs) = app_with_bound_label(|v| format!("{:04}", v % 10), None);
    h.pump();
    assert!(label_of(&h).contains("0000"));
    let baseline = runs.get();
    let bounds_before = h.node_bounds_by_id("lbl").expect("laid out");

    let n: Signal<i64> = h.runtime().signal("n", || 0);
    n.set(h.runtime(), 1);
    h.pump();

    assert!(label_of(&h).contains("0001"), "the patch did not apply");
    assert_eq!(
        runs.get(),
        baseline,
        "the view closure re-ran — this took the rebuild path, not the patch path"
    );
    assert_eq!(
        h.node_bounds_by_id("lbl").expect("still laid out"),
        bounds_before,
        "a layout-neutral patch moved the box"
    );
    h.assert_view_coherent();
}

#[test]
fn a_size_changing_text_change_falls_back_to_a_rebuild() {
    // "9" -> "10" is one glyph wider, so the box must actually change.
    let (mut h, runs) = app_with_bound_label(|v| format!("{}", v + 9), None);
    h.pump();
    let baseline = runs.get();
    let before = h.node_bounds_by_id("lbl").expect("laid out");

    let n: Signal<i64> = h.runtime().signal("n", || 0);
    n.set(h.runtime(), 1);
    h.pump();

    assert!(label_of(&h).contains("10"), "the new text never appeared");
    assert!(
        runs.get() > baseline,
        "a width-changing text change must rebuild, not patch"
    );
    let after = h.node_bounds_by_id("lbl").expect("still laid out");
    assert!(
        after.width() > before.width(),
        "the box should have grown: {before:?} -> {after:?}"
    );
    h.assert_view_coherent();
}

/// The whole point of storing `auto_w`/`auto_h`: an axis the author fixed
/// cannot move, so a new measurement on it is not a reason to rebuild. Without
/// this distinction the fast path would almost never fire, since most strings
/// differ in width by a pixel or two.
#[test]
fn an_author_fixed_width_patches_even_when_the_text_gets_wider() {
    let (mut h, runs) = app_with_bound_label(|v| format!("{}", v + 9), Some(200.0));
    h.pump();
    let baseline = runs.get();
    let before = h.node_bounds_by_id("lbl").expect("laid out");

    let n: Signal<i64> = h.runtime().signal("n", || 0);
    n.set(h.runtime(), 1);
    h.pump();

    assert!(label_of(&h).contains("10"), "the patch did not apply");
    assert_eq!(
        runs.get(),
        baseline,
        "a fixed-width label rebuilt over a change that cannot move it"
    );
    assert_eq!(
        h.node_bounds_by_id("lbl").expect("still laid out"),
        before,
        "a fixed-width box moved"
    );
    h.assert_view_coherent();
}

/// Several bindings changing in one pump: all-or-nothing. If any one of them
/// would move layout, none may be patched — a frame with some nodes patched
/// and others stale is exactly what the two-phase commit exists to prevent.
#[test]
fn one_layout_moving_binding_forces_the_whole_pump_to_rebuild() {
    let runs = Rc::new(Cell::new(0u32));
    let r = runs.clone();
    let mut h = App::new(move |cx: &mut BuildCx| {
        r.set(r.get() + 1);
        let n: Signal<i64> = cx.signal("n", || 0);
        let (a, b) = (n, n);
        widgets::column(vec![
            // stable width
            widgets::text("x")
                .id("stable")
                .bind_text(Dynamic::new(move |rt| format!("{:04}", a.get(rt) % 10))),
            // grows from "9" to "10"
            widgets::text("y")
                .id("grows")
                .bind_text(Dynamic::new(move |rt| format!("{}", b.get(rt) + 9))),
        ])
    })
    .run_headless(Size::new(300.0, 120.0));
    h.pump();
    let baseline = runs.get();

    let n: Signal<i64> = h.runtime().signal("n", || 0);
    n.set(h.runtime(), 1);
    h.pump();

    let json = label_of(&h);
    assert!(
        json.contains("0001") && json.contains("10"),
        "both must update: {json}"
    );
    assert!(
        runs.get() > baseline,
        "a pump containing a layout-moving binding must rebuild wholesale"
    );
    h.assert_view_coherent();
}

/// F3.6: the reason step 1 exists. A span containing a bound label used to be
/// marked `impure` and could never be spliced, so one binding anywhere in a
/// list forced the whole list to re-lower on every rebuild — the cost that
/// made bindings not worth using. Now the binding is settled before the build
/// chooses, and the span splices like any other.
#[test]
fn a_span_containing_a_bound_label_is_still_spliceable() {
    let mut h = App::new(move |cx: &mut BuildCx| {
        // `rows` is structural: writing it forces a rebuild. `n` drives a
        // binding inside the memoized span.
        let rows: Signal<usize> = cx.signal("rows", || 3);
        let n: Signal<i64> = cx.signal("n", || 0);
        let count = rows.get(cx.runtime());
        let mut kids = vec![widgets::text(format!("{count} rows"))];
        kids.push(cx.scope("card", move |_cx| {
            widgets::column(vec![
                widgets::text("static").id("s"),
                widgets::text("x")
                    .id("bound")
                    .bind_text(Dynamic::new(move |rt| format!("{:04}", n.get(rt) % 10000))),
            ])
        }));
        widgets::column(kids)
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();

    // A structural write elsewhere forces a rebuild; the card must splice.
    let rows: Signal<usize> = h.runtime().signal("rows", || 3);
    rows.set(h.runtime(), 4);
    let stats = h.pump();
    assert!(
        stats.nodes_copied > 0,
        "the span holding a bound label was re-lowered instead of spliced \
         (copied {} / rebuilt {})",
        stats.nodes_copied,
        stats.nodes_rebuilt
    );
    h.assert_view_coherent();
}

/// The hard case for F3.6: a binding changes *and* something structural
/// changes in the same pump. The pump rebuilds, so the patch path never runs —
/// the spliced span must still come back with the new value, which is what
/// `settle_bindings_for_rebuild` is for.
#[test]
fn a_binding_that_changes_during_a_rebuild_is_not_left_stale() {
    let mut h = App::new(move |cx: &mut BuildCx| {
        let rows: Signal<usize> = cx.signal("rows", || 3);
        let n: Signal<i64> = cx.signal("n", || 0);
        let count = rows.get(cx.runtime());
        widgets::column(vec![
            widgets::text(format!("{count} rows")),
            cx.scope("card", move |_cx| {
                widgets::text("x")
                    .id("bound")
                    .bind_text(Dynamic::new(move |rt| format!("{:04}", n.get(rt) % 10000)))
            }),
        ])
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    assert!(label_of(&h).contains("0000"));

    // Both writes land in the same pump: one structural, one a binding.
    let rows: Signal<usize> = h.runtime().signal("rows", || 3);
    let n: Signal<i64> = h.runtime().signal("n", || 0);
    rows.set(h.runtime(), 4);
    n.set(h.runtime(), 7);
    h.pump();

    assert!(
        label_of(&h).contains("0007"),
        "the spliced span kept the binding's old value: {}",
        label_of(&h)
    );
    h.assert_view_coherent();
}

/// Same collision, but the binding's new value is WIDER. It cannot be settled
/// in place, so the caches must be dropped and the node re-lowered against
/// fresh layout — the correctness fallback, not an optimisation.
#[test]
fn a_size_changing_binding_during_a_rebuild_relowers_and_resizes() {
    let mut h = App::new(move |cx: &mut BuildCx| {
        let rows: Signal<usize> = cx.signal("rows", || 3);
        let n: Signal<i64> = cx.signal("n", || 0);
        let count = rows.get(cx.runtime());
        widgets::column(vec![
            widgets::text(format!("{count} rows")),
            cx.scope("card", move |_cx| {
                widgets::text("x")
                    .id("bound")
                    .bind_text(Dynamic::new(move |rt| format!("{}", n.get(rt) + 9)))
            }),
        ])
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    let before = h.node_bounds_by_id("bound").expect("laid out");

    let rows: Signal<usize> = h.runtime().signal("rows", || 3);
    let n: Signal<i64> = h.runtime().signal("n", || 0);
    rows.set(h.runtime(), 4);
    n.set(h.runtime(), 1); // "9" -> "10"
    h.pump();

    assert!(label_of(&h).contains("10"), "the new value never appeared");
    let after = h.node_bounds_by_id("bound").expect("still laid out");
    assert!(
        after.width() > before.width(),
        "the box did not grow with the wider text: {before:?} -> {after:?}"
    );
    h.assert_view_coherent();
}

/// The gap the ablations found: every test above changes a binding *before* its
/// span is ever spliced. If the binding records were dropped for spliced spans,
/// the binding would simply stop being tracked — no panic, no stale frame at
/// the time, just a label that silently never updates again. This drives the
/// order that exposes it: splice first, then change the binding.
#[test]
fn a_binding_still_updates_after_its_span_has_been_spliced() {
    let mut h = App::new(move |cx: &mut BuildCx| {
        let rows: Signal<usize> = cx.signal("rows", || 3);
        let n: Signal<i64> = cx.signal("n", || 0);
        let count = rows.get(cx.runtime());
        widgets::column(vec![
            widgets::text(format!("{count} rows")),
            cx.scope("card", move |_cx| {
                widgets::text("x")
                    .id("bound")
                    .bind_text(Dynamic::new(move |rt| format!("{:04}", n.get(rt) % 10000)))
            }),
        ])
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();

    // 1. Structural change elsewhere → rebuild → the card splices.
    let rows: Signal<usize> = h.runtime().signal("rows", || 3);
    rows.set(h.runtime(), 4);
    let stats = h.pump();
    assert!(stats.nodes_copied > 0, "the card did not splice");

    // 2. NOW change the binding. Its record had to survive step 1 for this to
    //    be noticed at all.
    let n: Signal<i64> = h.runtime().signal("n", || 0);
    n.set(h.runtime(), 5);
    h.pump();
    assert!(
        label_of(&h).contains("0005"),
        "the binding stopped updating after its span was spliced: {}",
        label_of(&h)
    );

    // And again, to be sure the record is still there after a patch too.
    n.set(h.runtime(), 6);
    h.pump();
    assert!(label_of(&h).contains("0006"), "second update lost");
    h.assert_view_coherent();
}
