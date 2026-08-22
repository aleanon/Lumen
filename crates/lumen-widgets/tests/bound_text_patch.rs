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
