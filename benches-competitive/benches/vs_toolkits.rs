//! BENCH3 — Lumen vs **masonry** (Xilem's widget layer) on full-frame cost.
//!
//! Companion to `vs_iced.rs`; same sizes, same 400x800 viewport, same Lumen
//! side. Read that file's header for the methodology.
//!
//! # Why masonry rather than xilem
//!
//! `xilem` is the reactive view layer; `masonry` is the widget tree, layout,
//! paint and accessibility beneath it, and it is the part with a headless
//! harness (`TestHarness`, the one Xilem's own test suite uses). So this
//! measures Xilem's *lower half*. That understates a full Xilem frame, which
//! also diffs a view tree — stated here rather than buried, because it is the
//! one asymmetry that flatters masonry against Lumen's `pump()`.
//!
//! # The stopping point, and how it is enforced
//!
//! `TestHarness::render()` calls `render_root.redraw()` — which builds a vello
//! `Scene` and an AccessKit tree update — and *then* rasterizes that scene on
//! the GPU through vello. Rasterizing would repeat the egui mistake of
//! charging one side for work the other never does.
//!
//! masonry reads `SKIP_RENDER_TESTS`, and when it is set `render()` returns a
//! 1x1 placeholder **after** `redraw()` and the AccessKit update. That is the
//! matched stopping point: scene built, accessibility tree updated, nothing
//! rasterized — the direct counterpart of Lumen's display list plus semantics
//! tree under `NullRenderer`.
//!
//! The bench sets that variable itself (see `main`), so the measurement cannot
//! silently drift into including a GPU round trip.
//!
//! # Fairness note masonry earns
//!
//! Unlike iced, masonry **does** maintain an accessibility tree every frame,
//! so this is the first comparison where Lumen is not paying for a11y alone.

use criterion::{criterion_group, criterion_main, Criterion};
use kurbo::Size as KSize;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App};
// `Widget` is imported for its `with_auto_id()` method, not named directly.
use masonry::core::{NewWidget, Widget, WidgetTag};
use masonry::testing::TestHarness;
use masonry::theme::default_property_set;
use masonry::widgets::{Flex, Label};

const SIZES: [usize; 6] = [100, 250, 500, 1000, 2000, 3000];
const VIEW_W: f64 = 400.0;
const VIEW_H: f64 = 800.0;

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

fn lumen_frame(c: &mut Criterion) {
    let mut g = c.benchmark_group("frame/lumen");
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
        .run_headless(KSize::new(VIEW_W, VIEW_H));
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

fn masonry_frame(c: &mut Criterion) {
    let mut g = c.benchmark_group("frame/masonry");
    for n in SIZES {
        let tag: WidgetTag<Label> = WidgetTag::new("counter");
        let mut flex = Flex::column().with_child(NewWidget::new_with_tag(
            Label::new("counter: 0"),
            tag,
        ));
        for i in 1..n {
            flex = flex.with_child(Label::new(format!("row {i}")).with_auto_id());
        }
        let root = flex.with_auto_id();
        let mut h = TestHarness::create_with_size(
            default_property_set(),
            root,
            masonry::kurbo::Size::new(VIEW_W, VIEW_H),
        );
        let _ = h.render(); // warm, as the Lumen side is warmed by its first pump
        let mut bump = 0i64;
        g.bench_function(format!("{n}_rows"), |b| {
            b.iter(|| {
                bump += 1;
                h.edit_widget(tag, |mut l| {
                    Label::set_text(&mut l, format!("counter: {bump}"));
                });
                let _ = h.render();
            });
        });
    }
    g.finish();
}

fn main() {
    // Enforced here, not left to the caller: without it `render()` submits the
    // vello scene to the GPU and this stops measuring the same thing as the
    // Lumen side. See the header.
    // SAFETY: single-threaded, before any bench thread starts.
    unsafe { std::env::set_var("SKIP_RENDER_TESTS", "1") };
    benches();
    Criterion::default().configure_from_args().final_summary();
}

/// Isolates the accessibility tree, which BENCH2 and BENCH3 both assumed Lumen
/// pays for on every frame. It does not: `sem_root()` is lazy (OB2), and
/// `pump()` never calls it — every caller is a query path
/// (`semantics_doc`, `semantics_elided`, node lookup). So the `frame/lumen`
/// group above measures Lumen WITHOUT an accessibility tree.
///
/// This group forces one per frame. The difference between the two is the
/// real cost of the feature, and it is the number both earlier reports
/// reasoned about without measuring.
fn lumen_frame_with_semantics(c: &mut Criterion) {
    let mut g = c.benchmark_group("frame/lumen+semantics");
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
        .run_headless(KSize::new(VIEW_W, VIEW_H));
        h.pump();
        let sig: Signal<i64> = h.runtime().signal("n", || 0);
        g.bench_function(format!("{n}_rows"), |b| {
            b.iter(|| {
                sig.update(h.runtime(), |v| *v += 1);
                h.pump();
                // Forces the lazy build that pump() alone skips.
                std::hint::black_box(h.semantics_doc());
            });
        });
    }
    g.finish();
}

criterion_group!(benches, lumen_frame, lumen_frame_with_semantics, masonry_frame);
