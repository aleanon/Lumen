//! N0 measurement suite — the falsifiers for the retired `docs/plan-node-cost.md`; the surviving plan is `docs/plan-incremental-path.md`.
//!
//! The N-plan's priority order rests on one claim: *per-node lowering dominates
//! a changed frame and is allocation-bound*. A 2026-08-04 performance review
//! found that claim unfalsifiable with the instruments in the repo, and found
//! `scope_memo_one_of_many` (already in `perf.rs`) arguing against it. This file
//! adds the measurements that settle it.
//!
//! Three instruments, in order of how decisive they are:
//!
//!   * `allocs_per_frame` — allocations and bytes for one changed frame, by
//!     shape. **Deterministic** (unlike wall clock), so it answers
//!     "allocation-bound?" as arithmetic rather than inference: if a 500-node
//!     frame does A allocations and `A × ~25 ns` accounts for the lowering
//!     time, the thesis holds; if it doesn't, lowering is hashing or cloning
//!     and the N1 SoA refactor is aimed at the wrong thing.
//!   * `text_list_scoped_changed_frame` — 500 rows, per-row `cx.scope`, **one**
//!     row dirty. This is N3's literal acceptance criterion and no bench for it
//!     existed. Its ratio against the all-dirty `text_list_changed_frame` *is*
//!     the O(changed) property; a ratio near 1.0 falsifies the incremental model.
//!   * `scope_scaling` — fixed node count, varying scope count. `copy_span`
//!     (`app.rs:2754`) filters all `prev_spans` with a linear `contains` per
//!     copied span, i.e. O(scopes² × span). Every cost model in both plans is
//!     linear in *nodes*; this term is superlinear in *scopes*, which is exactly
//!     what the F-series encourages authors to add.

use criterion::{criterion_group, criterion_main, Criterion};
use kurbo::Size;
use lumen_core::color::Color;
use lumen_core::identity::{fold_id, hash_id, ScopePath};
use lumen_core::state::{Runtime, Signal};
use lumen_layout::{Dim, LayoutStyle};
use lumen_widgets::{widgets, App};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

// --- counting allocator (same shape as benches/identity.rs) -----------------

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(l.size(), Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(new.saturating_sub(l.size()), Ordering::Relaxed);
        unsafe { System.realloc(p, l, new) }
    }
}

#[global_allocator]
static A: Counting = Counting;

/// (allocations, bytes) performed while running `f`.
fn cost_of(f: impl FnOnce()) -> (usize, usize) {
    let a0 = ALLOCS.load(Ordering::Relaxed);
    let b0 = BYTES.load(Ordering::Relaxed);
    f();
    (
        ALLOCS.load(Ordering::Relaxed) - a0,
        BYTES.load(Ordering::Relaxed) - b0,
    )
}

// --- app shapes -------------------------------------------------------------

/// N flat text rows, one of which reads a signal **at the root** — so any write
/// re-runs the whole closure. This is `text_list_changed_frame`'s shape.
fn flat_app(n: i64) -> App {
    App::new(move |cx| {
        let bump = cx.signal("n", || 0i64).get(cx.runtime());
        let rows: Vec<_> = (0..n)
            .map(|i| {
                if i == 0 {
                    widgets::text(format!("counter: {bump}"))
                } else {
                    widgets::text(format!("row {i} (static)"))
                }
            })
            .collect();
        widgets::column(rows)
    })
}

/// N rows, each its own memoized `cx.scope` with a scope-local signal. Writing
/// one row's signal re-runs exactly that scope; the rest are memo hits that go
/// through the A.3.2 copy-forward path.
fn scoped_app(n: i64) -> App {
    App::new(move |cx| {
        let rows: Vec<_> = (0..n)
            .map(|i| {
                cx.scope(("row", i), move |cx| {
                    let s: Signal<i64> = cx.signal("v", || 0);
                    widgets::text(format!("row {i}: {}", s.get(cx.runtime())))
                })
            })
            .collect();
        widgets::column(rows)
    })
}

/// `rows_per_scope` text nodes inside each of `scopes` scopes — lets scope count
/// vary while total node count stays fixed.
fn banded_app(scopes: i64, rows_per_scope: i64) -> App {
    App::new(move |cx| {
        let bands: Vec<_> = (0..scopes)
            .map(|b| {
                cx.scope(("band", b), move |cx| {
                    let s: Signal<i64> = cx.signal("v", || 0);
                    let v = s.get(cx.runtime());
                    let rows: Vec<_> = (0..rows_per_scope)
                        .map(|r| widgets::text(format!("b{b} r{r}: {v}")))
                        .collect();
                    widgets::column(rows)
                })
            })
            .collect();
        widgets::column(bands)
    })
}

/// Address a scope-local signal from outside the build, the way the build folded
/// it (ADR-021). A flat string key would resolve to a *different*, root-level
/// signal that no scope reads — the hazard the ADR-021 log entry records as
/// having silently no-op'd `scope_memo_one_of_many`.
fn bump_scoped(rt: &Runtime, scope_key: (&'static str, i64)) {
    let scope = ScopePath::root().child(scope_key);
    let s: Signal<i64> = rt.signal_at(
        fold_id(scope.hash(), hash_id("v")),
        scope.hash(),
        || format!("{}-{}/v", scope_key.0, scope_key.1),
        || 0,
    );
    s.update(rt, |v| *v += 1);
}

// --- instrument 1: allocations per frame ------------------------------------

fn allocs_per_frame(c: &mut Criterion) {
    const N: i64 = 500;

    // Flat, all-dirty (the `text_list_changed_frame` shape).
    let mut flat = flat_app(N).run_headless(Size::new(400.0, 400.0));
    for _ in 0..4 {
        flat.pump();
    }
    // Warm once (first-touch interning, cache fills), then measure one frame.
    {
        let s: Signal<i64> = flat.runtime().signal("n", || 0);
        s.update(flat.runtime(), |v| *v += 1);
        flat.pump();
    }
    let (flat_a, flat_b) = cost_of(|| {
        let s: Signal<i64> = flat.runtime().signal("n", || 0);
        s.update(flat.runtime(), |v| *v += 1);
        flat.pump();
    });

    // Scoped, one-dirty (N3's target shape).
    let mut scoped = scoped_app(N).run_headless(Size::new(400.0, 400.0));
    for _ in 0..4 {
        scoped.pump();
    }
    bump_scoped(scoped.runtime(), ("row", 0));
    scoped.pump();
    let (scoped_a, scoped_b) = cost_of(|| {
        bump_scoped(scoped.runtime(), ("row", 1));
        scoped.pump();
    });

    // Idle (no write): the floor.
    let (idle_a, idle_b) = cost_of(|| {
        scoped.pump();
    });

    // ~25 ns is a generous per-allocation figure for glibc malloc on a warm
    // arena; if the frame's allocation count can't account for the phase time,
    // "allocation-bound" is the wrong diagnosis.
    let est_flat_us = flat_a as f64 * 25.0 / 1000.0;
    let est_scoped_us = scoped_a as f64 * 25.0 / 1000.0;

    println!(
        "\n\
         N0 — allocations per changed frame ({N} nodes)\n\
         ─────────────────────────────────────────────────────────────\n  \
         flat, all {N} dirty   : {flat_a:>7} allocs  {:>9} KiB  → ~{est_flat_us:.0} µs at 25 ns/alloc\n  \
         scoped, 1 of {N} dirty: {scoped_a:>7} allocs  {:>9} KiB  → ~{est_scoped_us:.0} µs at 25 ns/alloc\n  \
         idle pump            : {idle_a:>7} allocs  {:>9} KiB\n  \
         ─────────────────────────────────────────────────────────────\n  \
         per-node (flat)      : {:.1} allocs/node\n  \
         scoped/flat ratio    : {:.3}  (O(changed) ⇒ should approach 1/{N} = {:.3})\n",
        flat_b / 1024,
        scoped_b / 1024,
        idle_b / 1024,
        flat_a as f64 / N as f64,
        scoped_a as f64 / flat_a.max(1) as f64,
        1.0 / N as f64,
    );

    // Keep criterion happy with a trivial timed body so the target reports.
    let mut g = c.benchmark_group("allocs");
    g.sample_size(10);
    let mut k = 0i64;
    g.bench_function("scoped_one_dirty_allocs", |b| {
        b.iter(|| {
            std::hint::black_box(cost_of(|| {
                bump_scoped(scoped.runtime(), ("row", k % N));
                k += 1;
                scoped.pump();
            }))
        })
    });
    g.finish();
}

// --- instrument 2: the missing O(changed) bench ------------------------------

/// N3's literal acceptance criterion: 500 per-row scopes, one row dirty.
/// Compare against `perf.rs::text_list_changed_frame` (same node count, all
/// dirty). The ratio is the O(changed) property.
fn text_list_scoped_changed_frame(c: &mut Criterion) {
    const N: i64 = 500;
    let mut h = scoped_app(N).run_headless(Size::new(400.0, 400.0));
    for _ in 0..4 {
        h.pump();
    }
    let mut i = 0i64;
    c.bench_function("text_list_scoped_changed_frame", |b| {
        b.iter(|| {
            bump_scoped(h.runtime(), ("row", i % N));
            i += 1;
            h.pump();
        });
    });
}

// --- instrument 4: what is actually in the frame? ----------------------------

/// The same 500-node all-dirty frame with **text** leaves vs **rect** leaves.
///
/// The N-plan's phase table attributes the frame to lowering and display-list
/// emission and drops the raster row entirely. `text_list_changed_frame`'s own
/// doc comment contradicts its inline comment on this point — one says
/// "raster-dominated (headless re-rasters every glyph)", the other says "small
/// frame → cheap raster; DL emission dominates". Both cannot be right.
///
/// Node count, tree shape, scope structure and signal traffic are identical
/// across the two; only the leaf content differs. The delta is therefore text
/// shaping + glyph raster, and the rect number is an upper bound on everything
/// the N-plan's phases actually address.
fn text_vs_rect_frame(c: &mut Criterion) {
    const N: i64 = 500;
    let mut g = c.benchmark_group("leaf_content_500_nodes");

    let mut text_h = flat_app(N).run_headless(Size::new(400.0, 400.0));
    for _ in 0..4 {
        text_h.pump();
    }
    g.bench_function("text_leaves", |b| {
        b.iter(|| {
            let s: Signal<i64> = text_h.runtime().signal("n", || 0);
            s.update(text_h.runtime(), |v| *v += 1);
            text_h.pump();
        })
    });

    let rect_app = App::new(move |cx| {
        let bump = cx.signal("n", || 0i64).get(cx.runtime());
        let rows: Vec<_> = (0..N)
            .map(|i| {
                widgets::column(vec![])
                    .style(LayoutStyle {
                        width: Dim::px(200.0),
                        height: Dim::px(if i == 0 {
                            10.0 + (bump % 3) as f32
                        } else {
                            10.0
                        }),
                        ..LayoutStyle::default()
                    })
                    .background(Color::srgb8(40, 40, 60, 255))
            })
            .collect();
        widgets::column(rows)
    });
    let mut rect_h = rect_app.run_headless(Size::new(400.0, 400.0));
    for _ in 0..4 {
        rect_h.pump();
    }
    g.bench_function("rect_leaves", |b| {
        b.iter(|| {
            let s: Signal<i64> = rect_h.runtime().signal("n", || 0);
            s.update(rect_h.runtime(), |v| *v += 1);
            rect_h.pump();
        })
    });
    g.finish();
}

// --- instrument 3: scope-count scaling (the copy_span O(S²) term) ------------

fn scope_scaling(c: &mut Criterion) {
    // Total nodes held ~constant at 600; only the scope count varies.
    const TOTAL: i64 = 600;
    let mut g = c.benchmark_group("scope_scaling_600_nodes");
    for &scopes in &[10i64, 50, 100, 200, 300] {
        let per = TOTAL / scopes;
        let mut h = banded_app(scopes, per).run_headless(Size::new(400.0, 400.0));
        for _ in 0..4 {
            h.pump();
        }
        let mut i = 0i64;
        g.bench_function(format!("{scopes}_scopes"), |b| {
            b.iter(|| {
                bump_scoped(h.runtime(), ("band", i % scopes));
                i += 1;
                h.pump();
            });
        });
    }
    g.finish();
}

// --- instrument 5 (CP0.6): the observability share of a frame ---------------

/// How much of a changed frame is spent on the agent/a11y surface.
///
/// This replaces a QUARANTINED figure. `docs/plan-node-cost.md` claimed
/// "semantics + dep_index = 125 us of a 776 us frame (16%)", but that table is
/// disclaimed by its own document (its rows sum to 994 us against a 773 us
/// measured frame). Nothing reproducible ever backed it, so the campaign
/// forbids exit criteria citing it — hence this bench.
///
/// It measures the difference between a frame whose semantics are consumed and
/// one whose are not, which is the honest form of the question: the tree is
/// built eagerly today, so the cost is paid whether or not anyone reads it.
fn semantics_share(c: &mut Criterion) {
    const N: i64 = 500;
    let mut g = c.benchmark_group("observability_share_500_nodes");

    // Baseline: pump only.
    let mut h = flat_app(N).run_headless(Size::new(400.0, 800.0));
    h.pump();
    let n: Signal<i64> = h.runtime().signal("n", || 0);
    g.bench_function("pump_only", |b| {
        b.iter(|| {
            n.update(h.runtime(), |v| *v += 1);
            h.pump();
        });
    });

    // Same frame, plus building the elided projection an agent/AT would read.
    // The delta is what observability costs a *consumer*; the eager
    // `build_semantics` inside pump is charged to both sides, which is why
    // making it lazy (OB2) is measured by pump_only moving, not by this delta.
    let mut h2 = flat_app(N).run_headless(Size::new(400.0, 800.0));
    h2.pump();
    let n2: Signal<i64> = h2.runtime().signal("n", || 0);
    g.bench_function("pump_and_read_semantics", |b| {
        b.iter(|| {
            n2.update(h2.runtime(), |v| *v += 1);
            h2.pump();
            std::hint::black_box(h2.semantics_elided());
        });
    });
    g.finish();
}

// --- instrument 6 (CP0.6): the restyle path -------------------------------

/// A hover transition, which was entirely unbenched.
///
/// `restyle_visual` is a distinct pump outcome from a rebuild: no closure runs,
/// but the whole semantics tree is rebuilt anyway (`app.rs`'s restyle branch).
/// Pointer motion across an interactive UI therefore pays a per-node cost that
/// no gate watched. It is also the path OB2's incremental-semantics work has to
/// improve, so it needs a number before that work starts.
fn restyle_hover_frame(c: &mut Criterion) {
    const N: i64 = 500;

    // Buttons, not text: hover only takes the restyle path if some node's
    // hovered state actually flips. An all-text tree makes PointerMove an idle
    // pump (~4 us here), i.e. a bench that measures nothing and would stay
    // green forever — worse than having no bench at all.
    let mut h = App::new(move |cx| {
        let rows: Vec<_> = (0..N)
            .map(|i| {
                let s: Signal<i64> = cx.signal("clicks", || 0);
                widgets::button(format!("row {i}"), move |rt| s.update(rt, |v| *v += 1))
                    .id(format!("row-{i}"))
            })
            .collect();
        widgets::column(rows)
    })
    .run_headless(Size::new(400.0, 800.0));
    h.pump();

    // Take the points from real laid-out bounds rather than guessing
    // coordinates — a guess that lands outside every button silently turns
    // this into an idle-pump benchmark.
    let a = h
        .node_bounds_by_id("row-0")
        .expect("row-0 laid out")
        .center();
    let b_pt = h
        .node_bounds_by_id("row-1")
        .expect("row-1 laid out")
        .center();

    // Guard the measurement: if hover stops flipping state, this bench
    // silently becomes an idle-pump benchmark again.
    h.inject(lumen_core::events::Event::PointerMove(
        lumen_core::events::PointerEvent::at(a),
    ));
    h.pump();
    fn any_hovered(n: &lumen_core::semantics::SemanticsNode) -> bool {
        n.states.contains(&lumen_core::semantics::State::Hovered)
            || n.children.iter().any(any_hovered)
    }
    assert!(
        any_hovered(&h.semantics_elided()),
        "restyle_hover_frame must actually hover something, else it measures \
         an idle pump"
    );

    let mut flip = false;
    c.bench_function("restyle_hover_frame", |b| {
        b.iter(|| {
            flip = !flip;
            h.inject(lumen_core::events::Event::PointerMove(
                lumen_core::events::PointerEvent::at(if flip { a } else { b_pt }),
            ));
            h.pump();
        });
    });
}

criterion_group!(
    nodecost,
    allocs_per_frame,
    text_vs_rect_frame,
    text_list_scoped_changed_frame,
    scope_scaling,
    semantics_share,
    restyle_hover_frame
);
criterion_main!(nodecost);
