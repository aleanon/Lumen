//! WT-EXP — the memory half of the direct-lowering prototype.
//!
//! `lowercost.rs` times the two paths; this one counts what they allocate and
//! how much is alive at the worst moment. Both reach the same destination and
//! are checked with `lowered_eq` before anything is reported.

use lumen_widgets::direct::{begin_row, lower_element, lowered_eq, row_style, TreeSink};
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
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(new.saturating_sub(l.size()), Ordering::Relaxed);
        let live = LIVE.fetch_add(new.saturating_sub(l.size()), Ordering::Relaxed) + new - l.size();
        PEAK.fetch_max(live, Ordering::Relaxed);
        unsafe { System.realloc(p, l, new) }
    }
}

#[global_allocator]
static A: Counting = Counting;

const ROWS: usize = 500;

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

fn lower_via_element() -> TreeSink {
    let mut sink = TreeSink::new();
    let root = lumen_widgets::widgets::column(element_tree());
    lower_element(root, &mut sink, None);
    sink
}

fn lower_direct() -> TreeSink {
    let mut sink = TreeSink::new();
    let root = sink
        .node(None, lumen_core::semantics::Role::Group)
        .elide(true)
        .resolve()
        .index();
    let style = row_style(8.0, 4.0);
    let mut lns = Vec::with_capacity(ROWS);
    for i in 0..ROWS {
        let n = begin_row(&mut sink, Some(root));
        let (_, a) = lumen_widgets::direct::node(Label::new(format!("row {i}")).size(14.0))
            .lower(&mut sink, Some(n));
        let (_, b) = lumen_widgets::direct::node(ProgressBar::new(i as f64 / ROWS as f64))
            .lower(&mut sink, Some(n));
        let (_, c) = lumen_widgets::direct::node(Button::new("Open").ghost().on_press(|_| {}))
            .lower(&mut sink, Some(n));
        lns.push(sink.end(n, &style, &[a, b, c], false));
    }
    sink.end(root, &Default::default(), &lns, false);
    sink
}

/// (allocations, bytes, peak-live-bytes-added) while running `f`.
fn measure<T>(f: impl FnOnce() -> T) -> (T, usize, usize, usize) {
    let (a0, b0) = (
        ALLOCS.load(Ordering::Relaxed),
        BYTES.load(Ordering::Relaxed),
    );
    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let out = f();
    (
        out,
        ALLOCS.load(Ordering::Relaxed) - a0,
        BYTES.load(Ordering::Relaxed) - b0,
        PEAK.load(Ordering::Relaxed).saturating_sub(base),
    )
}

fn main() {
    // Warm the arena so first-touch growth is not attributed to either arm.
    drop(lower_via_element());
    drop(lower_direct());

    // Correctness gate first — a cheaper path that does less is not a result.
    let a = lower_via_element();
    let b = lower_direct();
    match lowered_eq(&a, &b) {
        Ok(()) => println!(
            "\nWT-EXP — direct lowering, {ROWS} rows ({} nodes). Paths agree.",
            a.tree.len()
        ),
        Err(e) => {
            eprintln!("paths disagree: {e}");
            std::process::exit(1);
        }
    }
    let nodes = a.tree.len();
    drop(a);
    drop(b);

    let (sink_a, alloc_a, bytes_a, peak_a) = measure(lower_via_element);
    drop(sink_a);
    let (sink_b, alloc_b, bytes_b, peak_b) = measure(lower_direct);
    drop(sink_b);

    // Where does the peak actually sit? Split the Element path in two: build
    // the staging tree, then walk it. If the staging buffer and the
    // destination were both fully alive at once, the combined peak would be
    // their sum. Measured separately, it is not.
    let ((tree, _), _, _, peak_build) = measure(|| (element_tree(), ()));
    let (_, _, _, peak_walk) = measure(|| {
        let mut sink = TreeSink::new();
        let root = lumen_widgets::widgets::column(tree);
        lower_element(root, &mut sink, None);
        sink
    });
    println!("\n  phase split (Element path)");
    println!(
        "    peak while BUILDING the staging tree : {:>6.2} MB",
        peak_build as f64 / 1048576.0
    );
    println!(
        "    peak while WALKING it into the sink  : {:>6.2} MB",
        peak_walk as f64 / 1048576.0
    );
    println!(
        "    sum if both were alive at once       : {:>6.2} MB",
        (peak_build + peak_walk) as f64 / 1048576.0
    );

    let el = std::mem::size_of::<Element>();
    println!("──────────────────────────────────────────────────────────────");
    println!("  size_of::<Element>()       : {el:>9} B");
    println!(
        "  staging cost if materialized: {:>8.2} MB   ({nodes} x {el} B)",
        (nodes * el) as f64 / 1048576.0
    );
    println!();
    println!("  {:<26}{:>12}{:>12}", "", "via_element", "direct");
    println!("  {:<26}{:>12}{:>12}", "allocations", alloc_a, alloc_b);
    println!(
        "  {:<26}{:>11.2}{:>11.2}",
        "total bytes (MB)",
        bytes_a as f64 / 1048576.0,
        bytes_b as f64 / 1048576.0
    );
    println!(
        "  {:<26}{:>11.2}{:>11.2}",
        "PEAK live bytes (MB)",
        peak_a as f64 / 1048576.0,
        peak_b as f64 / 1048576.0
    );
    println!("──────────────────────────────────────────────────────────────");
    println!(
        "  peak reduction            : {:>8.2}x   ({:+.1}%)",
        peak_a as f64 / peak_b.max(1) as f64,
        (peak_b as f64 - peak_a as f64) / peak_a as f64 * 100.0
    );
    println!(
        "  allocation change         : {:>+8.1}%",
        (alloc_b as f64 - alloc_a as f64) / alloc_a as f64 * 100.0
    );
    println!();
}
