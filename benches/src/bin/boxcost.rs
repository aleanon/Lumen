//! Why removing `Element` buys less than the prototype promised.
//!
//! Two arms, same ~2000-node tree, one process each (the allocator-residue
//! lesson from `lowertime`): the `Element` staging tree, and boxed dynamic
//! children. Reports allocations and peak bytes so the *mechanism* is visible,
//! not just the wall clock.

use lumen_widgets::direct::{lower_element, node, Column, Direct, Node, TreeSink};
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
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        // MUST delegate. The default `GlobalAlloc::realloc` is
        // alloc + memcpy + dealloc, so a growing `Vec` is counted as a fresh
        // allocation every doubling AND actually copied. The element arm's
        // vectors hold 784-byte `Element`s; the boxed arm's hold 16-byte
        // pointers, so omitting this inflated the element arm on both counters
        // and on the clock. It is the same bug that made `arenacost` report a
        // 52% win before its control arm was checked against `lowertime`.
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        let grow = new.saturating_sub(l.size());
        BYTES.fetch_add(grow, Ordering::Relaxed);
        let live = LIVE.fetch_add(grow, Ordering::Relaxed) + grow;
        PEAK.fetch_max(live, Ordering::Relaxed);
        unsafe { System.realloc(p, l, new) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        let a = p as usize;
        if a >= A_BASE.load(Ordering::Relaxed) && a < A_END.load(Ordering::Relaxed) {
            return; // arena-owned; the destructor has already run
        }
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static A: Counting = Counting;

// --- the arena arm: bump-allocate the widget tree ---------------------------
static A_BASE: AtomicUsize = AtomicUsize::new(0);
static A_END: AtomicUsize = AtomicUsize::new(0);
static A_OFF: AtomicUsize = AtomicUsize::new(0);
const ARENA: usize = 4 << 20;

fn arena_init() {
    let b = Box::leak(vec![0u8; ARENA].into_boxed_slice());
    A_BASE.store(b.as_ptr() as usize, Ordering::Relaxed);
    A_END.store(b.as_ptr() as usize + ARENA, Ordering::Relaxed);
    A_OFF.store(b.as_ptr() as usize, Ordering::Relaxed);
}

fn arena_node<W: Direct + 'static>(w: W) -> Node {
    let l = Layout::new::<Option<W>>();
    let off = (A_OFF.load(Ordering::Relaxed) + l.align() - 1) & !(l.align() - 1);
    let end = off + l.size();
    assert!(end < A_END.load(Ordering::Relaxed), "arena exhausted");
    A_OFF.store(end, Ordering::Relaxed);
    unsafe {
        let p = off as *mut Option<W>;
        std::ptr::write(p, Some(w));
        Box::from_raw(p)
    }
}

fn via_arena() -> usize {
    A_OFF.store(A_BASE.load(Ordering::Relaxed), Ordering::Relaxed);
    let rows: Vec<Node> = (0..ROWS)
        .map(|i| {
            let kids: Vec<Node> = vec![
                arena_node(Label::new(format!("row {i}")).size(14.0)),
                arena_node(ProgressBar::new(i as f64 / ROWS as f64)),
                arena_node(Button::new("Open").ghost().on_press(|_| {})),
            ];
            arena_node(Column::new(kids).gap(8.0).padding(4.0))
        })
        .collect();
    let mut sink = TreeSink::new();
    arena_node(Column::new(rows)).lower_dyn(&mut sink, None);
    sink.tree.len()
}

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
    node(Column::new(rows)).lower_dyn(&mut sink, None);
    sink.tree.len()
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "element".into());
    // Warm the allocator so the measured pass is not the first of its shape.
    arena_init();
    match mode.as_str() {
        "arena" => {
            std::hint::black_box(via_arena());
            reset();
            let n = via_arena();
            report("arena  ", n);
        }
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
