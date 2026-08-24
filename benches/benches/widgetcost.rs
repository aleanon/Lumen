//! WT-EXP — the cost of the *typed widget layer* itself.
//!
//! The existing suites (`perf.rs`, `nodecost.rs`) drive the engine through the
//! low-level `widgets::text` / `widgets::column` helpers, so they barely touch
//! the typed structs (`Button`, `Label`, `Container`, …). This file exists to
//! isolate exactly what the Widget-trait experiment changes: the construction
//! of a widget value and its lowering to `Element`.
//!
//! Two groups, deliberately separated:
//!
//!   * `construct/*` — build N widgets, run their modifier chains, lower to
//!     `Element`, drop. No runtime, no layout, no paint. This is the *upper
//!     bound* on any win: if the trait doesn't move these, it moves nothing.
//!   * `frame/*` — a real headless App whose view is built out of typed widgets,
//!     pumped as an all-dirty changed frame. This is what the user actually
//!     feels, and it dilutes the widget layer with layout/paint/semantics.
//!
//! Reporting both is the point: a big `construct` win with a flat `frame` number
//! means the widget layer was never the bottleneck.

use criterion::{criterion_group, criterion_main, Criterion};
use kurbo::Size;
use lumen_core::state::Signal;
use lumen_layout::Align;
use lumen_widgets::{
    widgets, App, Button, Card, CheckBox, Chip, Container, Element, Label, ProgressBar,
};
use std::hint::black_box;

/// How many widgets each `construct` bench builds per iteration.
const N: usize = 1_000;

// --- group 1: pure construction ---------------------------------------------

/// A push button with the modifier chain a real call site writes. `ghost()`
/// deliberately *overwrites* the background `new()` just set — the redundant
/// write that eager lowering pays and deferred lowering does not.
fn construct(c: &mut Criterion) {
    let mut g = c.benchmark_group("construct");

    g.bench_function("button_1k", |b| {
        b.iter(|| {
            let v: Vec<Element> = (0..N)
                .map(|i| {
                    Button::new("Save")
                        .ghost()
                        .on_press(|_| {})
                        .id(format!("btn{i}"))
                        .into()
                })
                .collect();
            black_box(v.len())
        })
    });

    // The heaviest `text_style_mut()` path: five modifiers, each of which
    // currently re-matches `NodeContent` to reach the `TextStyle`.
    g.bench_function("label_1k", |b| {
        b.iter(|| {
            let v: Vec<Element> = (0..N)
                .map(|i| {
                    Label::new(format!("row {i}"))
                        .size(15.0)
                        .bold()
                        .color(lumen_core::Color::WHITE)
                        .line_height(1.4)
                        .letter_spacing(0.2)
                        .into()
                })
                .collect();
            black_box(v.len())
        })
    });

    // A container carries its children by value, so its builder chain moves the
    // most bytes of any widget in the set.
    g.bench_function("container_1k", |b| {
        b.iter(|| {
            let v: Vec<Element> = (0..N)
                .map(|_| {
                    Container::new(vec![widgets::text("a"), widgets::text("b")])
                        .row()
                        .gap(8.0)
                        .padding(6.0)
                        .align(Align::Center)
                        .width(320.0)
                        .into()
                })
                .collect();
            black_box(v.len())
        })
    });

    g.bench_function("card_1k", |b| {
        b.iter(|| {
            let v: Vec<Element> = (0..N)
                .map(|_| Card::new(vec![widgets::text("body")]).into())
                .collect();
            black_box(v.len())
        })
    });

    g.bench_function("chip_1k", |b| {
        b.iter(|| {
            let v: Vec<Element> = (0..N).map(|_| Chip::new("tag").into()).collect();
            black_box(v.len())
        })
    });

    g.bench_function("progress_1k", |b| {
        b.iter(|| {
            let v: Vec<Element> = (0..N)
                .map(|i| ProgressBar::new(i as f64 / N as f64).into())
                .collect();
            black_box(v.len())
        })
    });

    // The realistic blend: a row of three widgets inside a container, ×N.
    g.bench_function("mixed_1k", |b| {
        b.iter(|| {
            let v: Vec<Element> = (0..N)
                .map(|i| {
                    Container::new(vec![
                        Label::new(format!("item {i}")).size(14.0).into(),
                        Chip::new("new").into(),
                        Button::new("Open").on_press(|_| {}).into(),
                    ])
                    .row()
                    .gap(8.0)
                    .padding(4.0)
                    .into()
                })
                .collect();
            black_box(v.len())
        })
    });

    g.finish();
}

// --- group 2: end-to-end frames ---------------------------------------------

/// `rows` rows of `Container[Label, CheckBox, Button]`, all reading a root
/// signal so every frame is a full lowering pass (the `flat_app` shape from
/// `nodecost.rs`, but built from typed widgets instead of raw helpers).
fn widget_app(rows: i64) -> App {
    App::new(move |cx| {
        let bump = cx.signal("n", || 0i64).get(cx.runtime());
        let list: Vec<Element> = (0..rows)
            .map(|i| {
                let row = Container::new(vec![
                    Label::new(format!("row {i} · {bump}")).size(14.0).into(),
                    CheckBox::new(cx, &format!("c{i}"), "done").into(),
                    Button::new("Open").ghost().on_press(|_| {}).into(),
                ])
                .row()
                .gap(8.0)
                .padding(4.0);
                row.into()
            })
            .collect();
        widgets::column(list)
    })
}

fn frame(c: &mut Criterion) {
    let mut g = c.benchmark_group("frame");
    for rows in [100i64, 500] {
        let mut h = widget_app(rows).run_headless(Size::new(600.0, 800.0));
        for _ in 0..4 {
            h.pump();
        }
        g.bench_function(format!("widget_rows_{rows}"), |b| {
            b.iter(|| {
                let s: Signal<i64> = h.runtime().signal("n", || 0);
                s.update(h.runtime(), |v| *v += 1);
                h.pump();
            })
        });
    }
    g.finish();
}

criterion_group!(widgetcost, construct, frame);
criterion_main!(widgetcost);
