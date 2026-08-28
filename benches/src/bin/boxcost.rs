//! Why removing `Element` buys less than the prototype promised.
//!
//! Two arms, same ~2000-node tree, one process each (the allocator-residue
//! lesson from `lowertime`): the `Element` staging tree, and boxed dynamic
//! children. Reports allocations and peak bytes so the *mechanism* is visible,
//! not just the wall clock.

use lumen_widgets::direct::{lower_element, node, Column, Node, TreeSink};
use lumen_widgets::{Button, Element, Label, ProgressBar};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

struct Counting;
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(l.size(), Ordering::Relaxed);
        let live = LIVE.fetch_add(l.size(), Ordering::Relaxed) + l.size();
        PEAK.fetch_max(live, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static A: Counting = Counting;

const ROWS: usize = 500;

fn reset() {
    ALLOCS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
}

fn report(what: &str, nodes: usize) {
    let a = ALLOCS.load(Ordering::Relaxed);
    let b = BYTES.load(Ordering::Relaxed);
    let p = PEAK.load(Ordering::Relaxed);
    println!(
        "{what}\tnodes={nodes}\tallocs={a}\t({:.2}/node)\tbytes={:.2} MB\tpeak={:.2} MB",
        a as f64 / nodes as f64,
        b as f64 / 1048576.0,
        p as f64 / 1048576.0
    );
}

fn via_element() -> usize {
    let rows: Vec<Element> = (0..ROWS)
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
        .collect();
    let mut sink = TreeSink::new();
    lower_element(lumen_widgets::widgets::column(rows), &mut sink, None);
    sink.tree.len()
}

fn via_boxed() -> usize {
    let rows: Vec<Node> = (0..ROWS)
        .map(|i| {
            let kids: Vec<Node> = vec![
                node(Label::new(format!("row {i}")).size(14.0)),
                node(ProgressBar::new(i as f64 / ROWS as f64)),
                node(Button::new("Open").ghost().on_press(|_| {})),
            ];
            node(Column::new(kids).gap(8.0).padding(4.0))
        })
        .collect();
    let mut sink = TreeSink::new();
    node(Column::new(rows)).lower(&mut sink, None);
    sink.tree.len()
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "element".into());
    // Warm the allocator so the measured pass is not the first of its shape.
    match mode.as_str() {
        "boxed" => {
            std::hint::black_box(via_boxed());
            reset();
            let n = via_boxed();
            report("boxed  ", n);
        }
        _ => {
            std::hint::black_box(via_element());
            reset();
            let n = via_element();
            report("element", n);
        }
    }
}
