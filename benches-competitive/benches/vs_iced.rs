//! BENCH2 — Lumen vs **iced** on full-view build time.
//!
//! Companion to `vs_egui.rs`, and it inherits that file's methodology whole:
//! same sizes, same 400x800 viewport, same Lumen side, same matched-stopping-
//! point discipline. Read its header first; only the differences are restated.
//!
//! # Why iced is a more informative comparison than egui
//!
//! egui is immediate-mode, so it legitimately skips the reconciliation Lumen
//! does on purpose — a real difference, but an architectural one that no amount
//! of optimisation closes. iced is **retained and reactive** like Lumen: it
//! builds a widget tree, diffs it against the previous one, lays it out, then
//! draws. That makes it the closer peer, and a gap against it is an
//! optimisation backlog rather than a category difference.
//!
//! # The stopping point
//!
//! * **Lumen** — `pump()` with `NullRenderer`: build, reconcile, lay out
//!   (taffy), paint to a display list, rebuild the semantics tree. Stops at the
//!   display list.
//! * **iced** — rebuild the `Element` tree, `Tree::diff` it, `layout()`, then
//!   `draw()` into an `iced_tiny_skia::Renderer`. Stops at that renderer's
//!   primitive layers.
//!
//! Neither rasterizes and neither submits to a GPU.
//!
//! # The trap this file exists to avoid
//!
//! `iced_core` ships a null renderer — `impl Renderer for ()` — and it is the
//! obvious thing to reach for. It sets `type Paragraph = ()`, so **text shaping
//! becomes a no-op**. An N-row *text* list benchmarked against it would compare
//! Lumen's parley/swash shaping against iced doing nothing, and would have
//! produced a flattering number that meant nothing. `iced_tiny_skia` is used
//! instead because its `text::Renderer` has a real cosmic-text `Paragraph`, so
//! both sides shape every row. This is the same failure the egui harness
//! already made once, in the other direction.
//!
//! # Unfairness, both directions
//!
//! **Against Lumen:** it rebuilds the semantics tree (accessibility + agent
//! surface) every frame; iced has no equivalent, so Lumen pays for a feature
//! iced does not offer.
//!
//! **Against iced:** `Tree::diff` is called every iteration, which is what a
//! real iced app does, but the widget tree here is structurally identical
//! frame to frame, so diffing is its cheapest case. Lumen's reconciliation is
//! likewise in its cheap case. Symmetric.
//!
//! **Neither culls.** Both lay out all N rows in a 400x800 viewport.

use criterion::{criterion_group, criterion_main, Criterion};
use iced_core::{
    layout::{self, Limits},
    mouse, renderer as core_renderer,
    widget::Tree,
    Element, Font, Length, Pixels, Rectangle, Theme,
};
use kurbo::Size as KSize;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App};

/// Identical to `vs_egui.rs` so the two tables can be read side by side.
const SIZES: [usize; 8] = [100, 250, 500, 750, 1000, 1400, 2000, 3000];

/// Sizes for the churn groups. Fewer points: the churn measurement is about
/// the SHAPE of the difference between steady state and worst case, and four
/// points across a 30x range show that without doubling an already long run.
const CHURN_SIZES: [usize; 4] = [100, 500, 1000, 3000];
const VIEW_W: f32 = 400.0;
const VIEW_H: f32 = 800.0;

struct NullRenderer;

impl lumen_render::Renderer for NullRenderer {
    fn render_frame(
        &mut self,
        _list: &lumen_render::DisplayList,
        _width: u32,
        _height: u32,
        _scale: f64,
        _background: lumen_core::Color,
    ) -> lumen_render::RgbaImage {
        lumen_render::RgbaImage::from_raw(0, 0, Vec::new())
    }

    fn name(&self) -> &'static str {
        "null"
    }
}

/// Steady state: ONE row's text changes per frame.
///
/// This is the case a real app spends its time in, and it is also the case
/// both frameworks' text caches are built for — which is exactly why the
/// `churn_*` groups below exist.
fn lumen_frame(c: &mut Criterion) {
    let mut g = c.benchmark_group("build_frame/lumen");
    for n in SIZES {
        let mut h = App::new(move |cx| {
            let bump = cx.signal("n", || 0i64).get(cx.runtime());
            let rows: Vec<_> = (0..n)
                .map(|i| {
                    if i == 0 {
                        widgets::text(format!("counter: {bump}"))
                    } else {
                        widgets::text(format!("row {i}"))
                    }
                })
                .collect();
            widgets::column(rows)
        })
        .with_renderer(NullRenderer)
        .run_headless(KSize::new(VIEW_W as f64, VIEW_H as f64));
        h.pump();
        let sig: Signal<i64> = h.runtime().signal("n", || 0);

        g.bench_function(format!("{n}_rows"), |b| {
            b.iter(|| {
                sig.update(h.runtime(), |v| *v += 1);
                h.pump();
            });
        });
    }
    g.finish();
}

type IcedElement<'a> = Element<'a, (), Theme, iced_tiny_skia::Renderer>;

/// The same N rows, one of which carries a counter that changes per iteration.
fn iced_view<'a>(n: usize, bump: i64) -> IcedElement<'a> {
    let rows: Vec<IcedElement<'a>> = (0..n)
        .map(|i| {
            let s = if i == 0 {
                format!("counter: {bump}")
            } else {
                format!("row {i}")
            };
            iced_widget::text(s).into()
        })
        .collect();
    iced_widget::Column::with_children(rows)
        .width(Length::Fixed(VIEW_W))
        .into()
}

fn iced_frame(c: &mut Criterion) {
    let mut g = c.benchmark_group("build_frame/iced");
    for n in SIZES {
        let mut renderer = iced_tiny_skia::Renderer::new(Font::default(), Pixels(16.0));
        let theme = Theme::Light;
        let style = core_renderer::Style::default();
        let viewport = Rectangle::new([0.0, 0.0].into(), [VIEW_W, VIEW_H].into());
        let limits = Limits::new([0.0, 0.0].into(), [VIEW_W, VIEW_H].into());

        // Warm the tree the way a running app has it warm.
        let first = iced_view(n, 0);
        let mut tree = Tree::new(&first);
        let mut bump = 0i64;

        g.bench_function(format!("{n}_rows"), |b| {
            b.iter(|| {
                bump += 1;
                let mut view = iced_view(n, bump);
                // What a live iced app does between frames.
                tree.diff(&view);
                let node = view.as_widget_mut().layout(&mut tree, &renderer, &limits);
                view.as_widget().draw(
                    &tree,
                    &mut renderer,
                    &theme,
                    &style,
                    layout::Layout::new(&node),
                    mouse::Cursor::Unavailable,
                    &viewport,
                );
            });
        });
    }
    g.finish();
}

/// Worst case: EVERY row's text changes per frame.
///
/// # Why this group is the important one
///
/// Both frameworks cache shaped text keyed by content — Lumen in `lumen-text`,
/// iced in the widget `Tree`'s paragraph state. With one row changing, 2999 of
/// 3000 rows hit that cache every frame, so `build_frame` is measuring each
/// framework's CACHE, not its text pipeline. The egui comparison learned this
/// the hard way: its headline gap "was never mainly about immediate-mode
/// versus retained — it was one framework's string cache working and the
/// other's defeating itself."
///
/// Changing every row denies both caches. The steady-state ratio and the churn
/// ratio together separate "our layout is slower" from "our cache is worse",
/// which a single number cannot.
fn lumen_churn(c: &mut Criterion) {
    let mut g = c.benchmark_group("churn_frame/lumen");
    for n in CHURN_SIZES {
        let mut h = App::new(move |cx| {
            let bump = cx.signal("n", || 0i64).get(cx.runtime());
            let rows: Vec<_> = (0..n)
                .map(|i| widgets::text(format!("row {i} v{bump}")))
                .collect();
            widgets::column(rows)
        })
        .with_renderer(NullRenderer)
        .run_headless(KSize::new(VIEW_W as f64, VIEW_H as f64));
        h.pump();
        let sig: Signal<i64> = h.runtime().signal("n", || 0);

        g.bench_function(format!("{n}_rows"), |b| {
            b.iter(|| {
                sig.update(h.runtime(), |v| *v += 1);
                h.pump();
            });
        });
    }
    g.finish();
}

fn iced_churn_view<'a>(n: usize, bump: i64) -> IcedElement<'a> {
    let rows: Vec<IcedElement<'a>> = (0..n)
        .map(|i| iced_widget::text(format!("row {i} v{bump}")).into())
        .collect();
    iced_widget::Column::with_children(rows)
        .width(Length::Fixed(VIEW_W))
        .into()
}

fn iced_churn(c: &mut Criterion) {
    let mut g = c.benchmark_group("churn_frame/iced");
    for n in CHURN_SIZES {
        let mut renderer = iced_tiny_skia::Renderer::new(Font::default(), Pixels(16.0));
        let theme = Theme::Light;
        let style = core_renderer::Style::default();
        let viewport = Rectangle::new([0.0, 0.0].into(), [VIEW_W, VIEW_H].into());
        let limits = Limits::new([0.0, 0.0].into(), [VIEW_W, VIEW_H].into());

        let first = iced_churn_view(n, 0);
        let mut tree = Tree::new(&first);
        let mut bump = 0i64;

        g.bench_function(format!("{n}_rows"), |b| {
            b.iter(|| {
                bump += 1;
                let mut view = iced_churn_view(n, bump);
                tree.diff(&view);
                let node = view.as_widget_mut().layout(&mut tree, &renderer, &limits);
                view.as_widget().draw(
                    &tree,
                    &mut renderer,
                    &theme,
                    &style,
                    layout::Layout::new(&node),
                    mouse::Cursor::Unavailable,
                    &viewport,
                );
            });
        });
    }
    g.finish();
}

criterion_group!(benches, lumen_frame, iced_frame, lumen_churn, iced_churn);
criterion_main!(benches);
