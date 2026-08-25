//! WT-EXP — does removing the `Element` staging record pay?
//!
//! Two paths to the *same destination* (SoA `Tree` + layout tree + per-node
//! side table):
//!
//!   * `via_element` — what the engine does today: widgets build `Element`s,
//!     then the lowering walk reads 41 fields back out of each one.
//!   * `direct` — the prototype: each widget writes its own fields into the
//!     sink; no `Element` is ever materialized.
//!
//! Both are timed on identical content, and `lowered_eq` asserts they produce
//! equivalent trees before either is measured — so a faster `direct` cannot be
//! an artefact of it doing less.

use criterion::{criterion_group, criterion_main, Criterion};
use lumen_widgets::direct::{begin_row, lower_element, lowered_eq, row_style, Direct, TreeSink};
use lumen_widgets::direct::{StyleEnv, VisualState};
use lumen_widgets::{Button, Element, Label, ProgressBar};
use std::hint::black_box;

/// Rows per lowering. One row is a container over a label, a progress bar and
/// a button — four nodes plus the bar's fill child, so five per row.
const ROWS: usize = 500;

// --- path A: build Elements, then walk them --------------------------------

fn element_tree() -> Vec<Element> {
    (0..ROWS)
        .map(|i| {
            let mut row: Element = lumen_widgets::Container::new(vec![
                Label::new(format!("row {i}")).size(14.0).into(),
                ProgressBar::new(i as f64 / ROWS as f64).into(),
                Button::new("Open").ghost().on_press(|_| {}).into(),
            ])
            .row()
            .gap(8.0)
            .padding(4.0)
            .into();
            row.elide_semantics = true;
            row
        })
        .collect()
}

fn lower_via_element(rows: Vec<Element>) -> TreeSink {
    let mut sink = TreeSink::new();
    let root = lumen_widgets::widgets::column(rows);
    lower_element(root, &mut sink, None);
    sink
}

// --- path B: widgets write straight in --------------------------------------

fn lower_direct() -> TreeSink {
    let mut sink = TreeSink::new();
    let root = sink.begin(None, lumen_core::semantics::Role::Group);
    let style = row_style(8.0, 4.0);
    let mut lns = Vec::with_capacity(ROWS);
    for i in 0..ROWS {
        let n = begin_row(&mut sink, Some(root));
        let (_, a) = Label::new(format!("row {i}")).size(14.0).lower(&mut sink, Some(n));
        let (_, b) = ProgressBar::new(i as f64 / ROWS as f64).lower(&mut sink, Some(n));
        let (_, c) = Button::new("Open")
            .ghost()
            .on_press(|_| {})
            .lower(&mut sink, Some(n));
        lns.push(sink.end(n, &style, &[a, b, c], false));
    }
    sink.end(root, &Default::default(), &lns, false);
    sink
}

// --- the benches ------------------------------------------------------------

fn lowering(c: &mut Criterion) {
    // Guard: the two paths must agree before either is timed.
    let a = lower_via_element(element_tree());
    let b = lower_direct();
    if let Err(e) = lowered_eq(&a, &b) {
        panic!("the two lowering paths disagree — the benchmark would be meaningless: {e}");
    }
    println!(
        "\nWT-EXP — lowering {ROWS} rows ({} nodes), both paths agree\n",
        a.tree.len()
    );

    let mut g = c.benchmark_group("lowering");

    // Building the Elements is part of today's cost, so it is inside the
    // measured region — that is the work `direct` removes.
    g.bench_function("via_element", |b| {
        b.iter(|| black_box(lower_via_element(element_tree()).tree.len()))
    });

    g.bench_function("direct", |b| {
        b.iter(|| black_box(lower_direct().tree.len()))
    });

    // Split out, so the marshalling and the construction are separable.
    g.bench_function("element_build_only", |b| {
        b.iter(|| black_box(element_tree().len()))
    });

    g.finish();
}

/// A stylesheet with the shapes that cost most to match: a type rule, a class
/// rule, a descendant selector and a state rule.
const SHEET: &str = "
button { border-radius: 4px; }
.fill { opacity: 1.0; }
group button { font-weight: 600; }
button:disabled { background: #cccccc; }
";

fn env() -> StyleEnv {
    let (sheet, _) = lumen_style::parse("bench.lss", SHEET);
    StyleEnv {
        sources: vec![lumen_style::StyleSource {
            sheet,
            origin: lumen_style::Origin::App,
        }],
        tokens: lumen_style::Tokens::default(),
        media: lumen_style::MediaContext::default(),
    }
}

fn lower_direct_styled() -> TreeSink {
    let mut sink = TreeSink::new().with_styles(env(), VisualState::default());
    let root = sink.begin(None, lumen_core::semantics::Role::Group);
    sink.resolve(root);
    let style = row_style(8.0, 4.0);
    let mut lns = Vec::with_capacity(ROWS);
    for i in 0..ROWS {
        let n = begin_row(&mut sink, Some(root));
        sink.resolve(n);
        let (_, a) = Label::new(format!("row {i}")).size(14.0).lower(&mut sink, Some(n));
        let (_, b) = ProgressBar::new(i as f64 / ROWS as f64).lower(&mut sink, Some(n));
        let (_, c) = Button::new("Open")
            .ghost()
            .on_press(|_| {})
            .lower(&mut sink, Some(n));
        lns.push(sink.end(n, &style, &[a, b, c], false));
    }
    sink.end(root, &Default::default(), &lns, false);
    sink
}

/// What the cascade costs on top of bare lowering, in the composed design.
fn styled(c: &mut Criterion) {
    let mut g = c.benchmark_group("cascade");
    g.bench_function("direct_unstyled", |b| {
        b.iter(|| black_box(lower_direct().tree.len()))
    });
    g.bench_function("direct_styled", |b| {
        b.iter(|| black_box(lower_direct_styled().tree.len()))
    });
    g.finish();
}

criterion_group!(lowercost, lowering, styled);
criterion_main!(lowercost);
