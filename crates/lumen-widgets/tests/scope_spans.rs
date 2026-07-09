//! A.3.1 (docs/plan-retained-pipeline.md): every `cx.scope` records its node
//! span — subtree root + preorder node count — the anchor the retained-graph
//! splice (A.3.3) will replace. Spans must cover exactly the scope's subtree
//! and refresh when the scope re-runs.

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{col, widgets, App};

#[test]
fn scope_spans_cover_exactly_the_subtree() {
    let mut h = App::new(|cx| {
        cx.signal("n", || 2usize);
        col![
            widgets::text("before"),
            cx.scope("list", |cx| {
                let n = cx.runtime().signal("n", || 2usize);
                let count = n.get(cx.runtime());
                col![
                    widgets::text(format!("{count} items")),
                    widgets::column(
                        (0..count)
                            .map(|i| widgets::text(format!("item {i}")))
                            .collect::<Vec<_>>()
                    ),
                ]
            }),
            widgets::text("after").id("after"),
        ]
    })
    .run_headless(Size::new(300.0, 300.0));
    h.pump();

    // The scope subtree: outer col + label + inner column + 2 items = 5.
    let (_root, nodes) = h.scope_span("list").expect("span recorded");
    assert_eq!(nodes, 5, "span counts the whole subtree");
    h.assert_view_coherent();
}

#[test]
fn scope_span_refreshes_when_the_scope_reruns() {
    let mut h = App::new(|cx| {
        cx.signal("n", || 1usize);
        col![cx.scope("items", |cx| {
            let n = cx.runtime().signal("n", || 1usize);
            let count = n.get(cx.runtime());
            widgets::column(
                (0..count)
                    .map(|i| widgets::text(format!("row {i}")))
                    .collect::<Vec<_>>(),
            )
        })]
    })
    .run_headless(Size::new(300.0, 300.0));
    h.pump();
    let (_, before) = h.scope_span("items").expect("span recorded");
    assert_eq!(before, 2, "column + 1 row");

    let n: Signal<usize> = h.runtime().signal("n", || 1usize);
    n.set(h.runtime(), 3);
    h.pump();
    let (_, after) = h.scope_span("items").expect("span survives re-run");
    assert_eq!(after, 4, "column + 3 rows after the signal write");
    h.assert_view_coherent();
}
