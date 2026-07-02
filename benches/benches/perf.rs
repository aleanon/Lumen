//! T2.6 perf benches. Budgets enforced by `scripts/perf_gate.sh`:
//!   * `layout_10k_dirty_subtree` < 2 ms
//!   * `vlist_1m_scroll`          < 8.33 ms (120 fps frame budget)
//!   * `idle_frame`               < 2 ms (idle does no real work)

use criterion::{criterion_group, criterion_main, Criterion};
use kurbo::{Point, Rect, Size, Vec2};
use lumen_core::events::{Event, Modifiers, WheelEvent};
use lumen_core::state::Runtime;
use lumen_layout::{Dim, LayoutStyle, LayoutTree};
use lumen_render::scene::cull_visible;
use lumen_widgets::{widgets, widgets_m1, widgets_m4, App};

/// 100k-node scene: cull a large scene against a viewport (multi-threaded, T6.6).
fn cull_100k(c: &mut Criterion) {
    let bounds: Vec<Rect> = (0u64..100_000)
        .map(|i| {
            let x = (i.wrapping_mul(2654435761) % 100_000) as f64;
            let y = (i.wrapping_mul(40503) % 100_000) as f64;
            Rect::new(x, y, x + 30.0, y + 20.0)
        })
        .collect();
    let viewport = Rect::new(10_000.0, 10_000.0, 30_000.0, 30_000.0);
    c.bench_function("cull_100k", |b| {
        b.iter(|| cull_visible(&bounds, viewport).len());
    });
}

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

/// A signal holding a large `Vec`; each iteration mutates one element via
/// `update`. Guards that `update` is O(1) in the value size — an in-place
/// mutation — rather than O(n) (a full clone of the value per write). A large
/// collection kept in one signal is a common large-app shape (e.g. a todo/row
/// list), so this is the write cost that scales with app state.
fn signal_update_large_vec(c: &mut Criterion) {
    let rt = Runtime::new();
    let sig = rt.signal("big", || vec![0u64; 100_000]);
    let mut i = 0usize;
    c.bench_function("signal_update_large_vec", |b| {
        b.iter(|| {
            sig.update(&rt, |v| {
                let n = v.len();
                v[i % n] = v[i % n].wrapping_add(1);
            });
            i += 1;
        });
    });
}

/// F1: a view of 200 memoized `cx.scope` rows where a single row's signal
/// changes each iteration. With scope memoization the rebuild re-runs only the
/// one changed scope (the other 199 reuse cached subtrees), so pump cost is
/// ~independent of the row count — the "fine-grained view update" property.
fn scope_memo_one_of_many(c: &mut Criterion) {
    use lumen_core::state::Signal;
    const N: i64 = 200;
    let app = App::new(|cx| {
        let rows: Vec<_> = (0..N)
            .map(|i| {
                cx.scope(&format!("row-{i}"), move |cx| {
                    let s: Signal<i64> = cx.signal(&format!("v-{i}"), || 0);
                    widgets::text(format!("row {i}: {}", s.get(cx.runtime())))
                })
            })
            .collect();
        widgets::column(rows)
    });
    let mut h = app.run_headless(Size::new(400.0, 600.0));
    let mut i = 0i64;
    c.bench_function("scope_memo_one_of_many", |b| {
        b.iter(|| {
            // Flip one row's signal, then pump — only that scope should re-run.
            let s: Signal<i64> = h.runtime().signal(&format!("v-{}", i % N), || 0);
            s.update(h.runtime(), |v| *v += 1);
            h.pump();
            i += 1;
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
    cull_100k,
    signal_update_large_vec,
    scope_memo_one_of_many,
    idle_frame
);
criterion_main!(perf);
