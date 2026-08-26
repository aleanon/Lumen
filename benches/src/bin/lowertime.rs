//! WT-EXP — timing the two lowering paths, one variant per process.
//!
//! criterion could not be trusted here. Both lowering paths allocate ~10 MB per
//! iteration, so their timings track the allocator's residual state rather than
//! the code: running the *same* `lower_direct` in two different benchmark
//! groups produced 941 µs and 2.71 ms, a 3× spread with nothing changed. Any
//! comparison drawn from one process that runs both arms is measuring heap
//! history.
//!
//! So each arm gets its own process, with identical startup, its own warmup,
//! and the median of many runs. Invoke as `lowertime <element|direct|styled>`;
//! `compare_lowering.sh` runs them interleaved and reports medians.

use lumen_widgets::direct::{
    begin_row, lower_element, row_style, Direct, StyleEnv, TreeSink, VisualState,
};
use lumen_widgets::{Button, Element, Label, ProgressBar};
use std::time::Instant;

const ROWS: usize = 500;
const WARMUP: usize = 20;
const SAMPLES: usize = 100;

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

fn via_element_styled(styles: Option<StyleEnv>) -> usize {
    let mut sink = match styles {
        None => TreeSink::new(),
        Some(env) => TreeSink::new().with_styles(env, VisualState::default()),
    };
    let root = lumen_widgets::widgets::column(element_tree());
    lower_element(root, &mut sink, None);
    sink.tree.len()
}

fn direct(styles: Option<StyleEnv>) -> usize {
    let mut sink = match styles {
        None => TreeSink::new(),
        Some(env) => TreeSink::new().with_styles(env, VisualState::default()),
    };
    let root = sink
        .node(None, lumen_core::semantics::Role::Group)
        .elide(true)
        .resolve()
        .index();
    sink.resolve(root);
    let style = row_style(8.0, 4.0);
    let mut lns = Vec::with_capacity(ROWS);
    for i in 0..ROWS {
        let n = begin_row(&mut sink, Some(root));
        sink.resolve(n);
        let (_, a) = Label::new(format!("row {i}"))
            .size(14.0)
            .lower(&mut sink, Some(n));
        let (_, b) = ProgressBar::new(i as f64 / ROWS as f64).lower(&mut sink, Some(n));
        let (_, c) = Button::new("Open")
            .ghost()
            .on_press(|_| {})
            .lower(&mut sink, Some(n));
        lns.push(sink.end(n, &style, &[a, b, c], false));
    }
    sink.end(root, &Default::default(), &lns, false);
    sink.tree.len()
}

const SHEET: &str = "
button { border-radius: 4px; }
.fill { opacity: 1.0; }
group button { font-weight: 600; }
button:disabled { background: #cccccc; }
";

fn env() -> StyleEnv {
    StyleEnv::from_source(SHEET).expect("parses")
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "direct".into());
    let run: Box<dyn Fn() -> usize> = match mode.as_str() {
        "element" => Box::new(|| via_element_styled(None)),
        "element_styled" => Box::new(|| via_element_styled(Some(env()))),
        "direct" => Box::new(|| direct(None)),
        "styled" => Box::new(|| direct(Some(env()))),
        other => {
            eprintln!("unknown mode {other}");
            std::process::exit(2);
        }
    };

    for _ in 0..WARMUP {
        std::hint::black_box(run());
    }
    let mut us: Vec<f64> = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let t = Instant::now();
        std::hint::black_box(run());
        us.push(t.elapsed().as_secs_f64() * 1e6);
    }
    us.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // Median, and the interquartile range as an honesty check on the spread.
    println!(
        "{mode}\t{:.1}\t{:.1}\t{:.1}",
        us[SAMPLES / 2],
        us[SAMPLES / 4],
        us[SAMPLES * 3 / 4]
    );
}
