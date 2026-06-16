//! T2.6 perf benches. Budgets enforced by `scripts/perf_gate.sh`:
//!   * `layout_10k_dirty_subtree` < 2 ms
//!   * `vlist_1m_scroll`          < 8.33 ms (120 fps frame budget)
//!   * `idle_frame`               < 2 ms (idle does no real work)

use criterion::{criterion_group, criterion_main, Criterion};
use kurbo::{Point, Size, Vec2};
use lumen_core::events::{Event, Modifiers, WheelEvent};
use lumen_layout::{Dim, LayoutStyle, LayoutTree};
use lumen_widgets::{widgets, widgets_m1, widgets_m4, App};

/// 10k-node tree: one container over 10 000 fixed-height leaves; the bench
/// recomputes the whole subtree each iteration.
fn layout_10k_dirty_subtree(c: &mut Criterion) {
    let leaf_style = LayoutStyle {
        width: Dim::px(200.0),
        height: Dim::px(20.0),
        ..LayoutStyle::default()
    };
    let mut tree = LayoutTree::new();
    let leaves: Vec<_> = (0..10_000).map(|_| tree.leaf(leaf_style.clone())).collect();
    let col = tree.container(LayoutStyle::default(), &leaves);
    tree.compute(col, Size::new(200.0, 200_000.0));
    assert!(tree.touched() >= 10_000);

    c.bench_function("layout_10k_dirty_subtree", |b| {
        b.iter(|| tree.relayout_subtree(col));
    });
}

/// 1M-row VirtualList: each iteration scrolls and pumps a frame. The window only
/// materialises the visible rows, so cost is independent of row count.
fn vlist_1m_scroll(c: &mut Criterion) {
    let app = App::new(|cx| {
        widgets_m1::virtual_list(cx, "vl", 1_000_000, 20.0, 600.0, |i| {
            widgets::text(format!("row {i}"))
        })
    });
    let mut h = app.run_headless(Size::new(400.0, 600.0));

    c.bench_function("vlist_1m_scroll", |b| {
        b.iter(|| {
            h.inject(Event::Wheel(WheelEvent {
                pos: Point::new(200.0, 300.0),
                delta: Vec2::new(0.0, 40.0),
                modifiers: Modifiers::empty(),
            }));
            h.pump();
        });
    });
}

/// 1M-row DataGrid: each iteration scrolls and pumps a frame. Like VirtualList
/// the grid windows its rows, so cost is independent of row count (T4.2 gate).
fn data_grid_1m_scroll(c: &mut Criterion) {
    let app = App::new(|cx| {
        widgets_m4::data_grid(
            cx,
            "grid",
            &["A", "B", "C"],
            1_000_000,
            20.0,
            600.0,
            |r, col| format!("r{r}c{col}"),
        )
    });
    let mut h = app.run_headless(Size::new(400.0, 600.0));

    c.bench_function("data_grid_1m_scroll", |b| {
        b.iter(|| {
            h.inject(Event::Wheel(WheelEvent {
                pos: Point::new(200.0, 300.0),
                delta: Vec2::new(0.0, 40.0),
                modifiers: Modifiers::empty(),
            }));
            h.pump();
        });
    });
}

/// Idle: pumping with no input queued. The scheduler should do no real work.
fn idle_frame(c: &mut Criterion) {
    let app = App::new(|cx| {
        let count = cx.signal("count", || 0i32);
        let v = count.get(cx.runtime());
        widgets::text(format!("Count: {v}"))
    });
    let mut h = app.run_headless(Size::new(200.0, 80.0));

    c.bench_function("idle_frame", |b| {
        b.iter(|| h.pump());
    });
}

criterion_group!(
    perf,
    layout_10k_dirty_subtree,
    vlist_1m_scroll,
    data_grid_1m_scroll,
    idle_frame
);
criterion_main!(perf);
