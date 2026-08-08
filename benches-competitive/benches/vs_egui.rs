//! BENCH1 — Lumen vs egui on **full-view build time**.
//!
//! The owner's performance bar is relative: **A = match the industry leader,
//! A+ = surpass it**. That cannot be graded without an external number, and
//! before this the project had never benchmarked against another framework —
//! its own review called that the single most conspicuous gap in the
//! competitive story.
//!
//! # What is measured
//!
//! One frame that builds an N-row text list and produces draw commands:
//!
//! * **Lumen** — `Headless::pump()` after a signal write: re-run the view
//!   closure, reconcile, lay out (taffy), paint to a display list, and rebuild
//!   the semantics tree.
//! * **egui** — `Context::run()` + `tessellate()`: re-run the UI closure, lay
//!   out, and tessellate shapes into meshes.
//!
//! Both therefore cover build → layout → "ready to hand to a renderer", and
//! neither includes GPU submission.
//!
//! # What makes this comparison unfair, in both directions
//!
//! Stated up front because a competitive number is worthless without them, and
//! because it is easy to quote the ratio and drop the caveats.
//!
//! **Against Lumen:**
//! * `pump()` also builds the **semantics tree** every frame — the
//!   accessibility/agent surface. egui has no equivalent, so Lumen is paying
//!   for a feature egui does not offer. (Making that lazy is OB2, unlanded.)
//! * Lumen shapes text with parley/swash (full Unicode, bidi-capable); egui
//!   uses its own simpler layout. Different capability, different cost.
//!
//! **Against egui:**
//! * egui is immediate-mode by design — it has no retained tree to reconcile,
//!   so it skips work Lumen does on purpose. Comparing them on a *full rebuild*
//!   is the case that flatters egui least-unfairly; comparing incremental
//!   updates would be meaningless, since egui has none.
//! * egui caches galley (text layout) results across frames keyed by string.
//!   With static row labels that cache hits every frame after the first, so
//!   this measures egui's *steady state*, which is its best case.
//!
//! Conclusion to draw: an order-of-magnitude read on whether Lumen is in the
//! same class. Do NOT read a 10-20% difference as meaningful.

use criterion::{criterion_group, criterion_main, Criterion};
use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App};

const SIZES: [usize; 3] = [100, 500, 2000];

/// Lumen: N rows, one of which reads a signal, so a write dirties the view.
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
        .run_headless(Size::new(400.0, 800.0));
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

/// egui: the same N rows, rebuilt and tessellated each iteration.
fn egui_frame(c: &mut Criterion) {
    let mut g = c.benchmark_group("build_frame/egui");
    for n in SIZES {
        let ctx = egui::Context::default();
        // Match Lumen's 400x800 logical viewport.
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 800.0),
            )),
            ..Default::default()
        };
        input.time = Some(0.0);

        // Warm the galley cache and the font atlas, so the first measured
        // iteration isn't paying one-time setup Lumen already paid above.
        let mut counter = 0i64;
        for _ in 0..2 {
            let out = ctx.run(input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    for i in 0..n {
                        if i == 0 {
                            ui.label(format!("counter: {counter}"));
                        } else {
                            ui.label(format!("row {i}"));
                        }
                    }
                });
            });
            let _ = ctx.tessellate(out.shapes, out.pixels_per_point);
        }

        g.bench_function(format!("{n}_rows"), |b| {
            b.iter(|| {
                // Change one label per iteration, mirroring Lumen's signal
                // write, so neither side can cache the whole frame away.
                counter += 1;
                let out = ctx.run(input.clone(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        for i in 0..n {
                            if i == 0 {
                                ui.label(format!("counter: {counter}"));
                            } else {
                                ui.label(format!("row {i}"));
                            }
                        }
                    });
                });
                std::hint::black_box(ctx.tessellate(out.shapes, out.pixels_per_point));
            });
        });
    }
    g.finish();
}

criterion_group!(competitive, lumen_frame, egui_frame);
criterion_main!(competitive);
