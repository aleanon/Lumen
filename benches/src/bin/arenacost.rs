//! Does bump-allocating the widget tree recover direct lowering's win?
//!
//! `boxcost` found that dynamic children cost **more** allocations per node
//! than the `Element` tree they replace (4.52 vs 4.32), which is why removing
//! `Element` measured at −6% rather than the −24% the prototype claimed. The
//! hypothesis: the per-node `Box` is eating the byte saving, and an arena that
//! bump-allocates the widget tree would give it back.
//!
//! This isolates exactly that. `arena_node` places a widget into a bump region
//! and hands back a `Box` over arena memory; the global allocator recognises
//! arena pointers by address range and skips their `dealloc` (the destructor
//! still runs — only the free is elided, because the arena owns the storage).
//! Everything else in the process keeps the system allocator, so the ONLY thing
//! that changes between the `boxed` and `arena` arms is where widget nodes come
//! from.
//!
//! One arm per process, per the allocator-residue lesson in `lowertime`.

use lumen_widgets::direct::{lower_element, node, Column, Direct, Node, TreeSink};
use lumen_widgets::{Button, Element, Label, ProgressBar};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

const ROWS: usize = 500;
const ARENA: usize = 4 << 20;
const WARMUP: usize = 20;
const SAMPLES: usize = 100;

static A_BASE: AtomicUsize = AtomicUsize::new(0);
static A_END: AtomicUsize = AtomicUsize::new(0);
static A_OFF: AtomicUsize = AtomicUsize::new(0);

struct Hybrid;
unsafe impl GlobalAlloc for Hybrid {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        // No counting here. An earlier revision incremented two atomics per
        // allocation and leaked a 64 MB arena up front; the CONTROL arm then
        // measured 2477 us against `lowertime`'s 1193 for the same work, and
        // was bimodal (min 1479, median 2562). A harness whose control arm
        // disagrees 2x with the established one is measuring itself.
        unsafe { System.alloc(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        // MUST be delegated. The default `GlobalAlloc::realloc` is
        // alloc + memcpy + dealloc, which turns every `Vec` growth into a full
        // copy instead of an in-place extension. Omitting it made the element
        // arm — whose vectors hold 784-byte `Element`s and grow by doubling —
        // measure 2477 us against `lowertime`'s 1193, while the boxed arm,
        // whose vectors hold 16-byte pointers, barely moved. That asymmetry
        // read as a 52% win for direct lowering and was entirely this bug.
        let a = p as usize;
        if a >= A_BASE.load(Ordering::Relaxed) && a < A_END.load(Ordering::Relaxed) {
            // Arena blocks are never grown; nothing allocates into one twice.
            unreachable!("realloc of arena memory");
        }
        unsafe { System.realloc(p, l, new) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        let a = p as usize;
        // Arena memory is owned by the arena; the value's destructor has
        // already run by the time we get here, so only the free is skipped.
        if a >= A_BASE.load(Ordering::Relaxed) && a < A_END.load(Ordering::Relaxed) {
            return;
        }
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static G: Hybrid = Hybrid;

fn arena_init() {
    let v = vec![0u8; ARENA].into_boxed_slice();
    let b = Box::leak(v);
    A_BASE.store(b.as_ptr() as usize, Ordering::Relaxed);
    A_END.store(b.as_ptr() as usize + ARENA, Ordering::Relaxed);
    A_OFF.store(b.as_ptr() as usize, Ordering::Relaxed);
}

fn arena_reset() {
    A_OFF.store(A_BASE.load(Ordering::Relaxed), Ordering::Relaxed);
}

/// Bump-allocate `w` and hand back a `Node` over arena memory.
fn arena_node<W: Direct + 'static>(w: W) -> Node {
    let l = Layout::new::<W>();
    let mut off = A_OFF.load(Ordering::Relaxed);
    off = (off + l.align() - 1) & !(l.align() - 1);
    let end = off + l.size();
    assert!(end < A_END.load(Ordering::Relaxed), "arena exhausted");
    A_OFF.store(end, Ordering::Relaxed);
    let p = off as *mut W;
    unsafe {
        std::ptr::write(p, w);
        Box::from_raw(p)
    }
}

fn element_tree() -> Element {
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
    lumen_widgets::widgets::column(rows)
}

fn via_element() -> usize {
    let mut sink = TreeSink::new();
    lower_element(element_tree(), &mut sink, None);
    sink.tree.len()
}

fn widget_tree(mk: fn(Column) -> Node, leaf: fn(Label) -> Node) -> Node {
    let rows: Vec<Node> = (0..ROWS)
        .map(|i| {
            let kids: Vec<Node> = vec![
                leaf(Label::new(format!("row {i}")).size(14.0)),
                mk_pb(i),
                mk_btn(),
            ];
            mk(Column::new(kids).gap(8.0).padding(4.0))
        })
        .collect();
    mk(Column::new(rows))
}

// The two leaf constructors are swapped per arm via these thread-locals, so the
// tree shape is byte-identical and only the allocation source differs.
static ARENA_MODE: AtomicUsize = AtomicUsize::new(0);
fn mk_pb(i: usize) -> Node {
    let w = ProgressBar::new(i as f64 / ROWS as f64);
    if ARENA_MODE.load(Ordering::Relaxed) == 1 {
        arena_node(w)
    } else {
        node(w)
    }
}
fn mk_btn() -> Node {
    let w = Button::new("Open").ghost().on_press(|_| {});
    if ARENA_MODE.load(Ordering::Relaxed) == 1 {
        arena_node(w)
    } else {
        node(w)
    }
}
fn mk_col(c: Column) -> Node {
    if ARENA_MODE.load(Ordering::Relaxed) == 1 {
        arena_node(c)
    } else {
        node(c)
    }
}
fn mk_label(l: Label) -> Node {
    if ARENA_MODE.load(Ordering::Relaxed) == 1 {
        arena_node(l)
    } else {
        node(l)
    }
}

fn via_widgets() -> usize {
    let mut sink = TreeSink::new();
    widget_tree(mk_col, mk_label).lower(&mut sink, None);
    sink.tree.len()
}

fn main() {
    // NOARENA=1 skips the 64 MB leak, to test whether the arena's own
    // allocation is what moves the element arm rather than anything measured.
    if std::env::var("NOARENA").is_err() {
        arena_init();
    }
    let mode = std::env::args().nth(1).unwrap_or_else(|| "element".into());
    let run: Box<dyn Fn() -> usize> = match mode.as_str() {
        "element" => Box::new(via_element),
        "boxed" => Box::new(via_widgets),
        "arena" => {
            ARENA_MODE.store(1, Ordering::Relaxed);
            Box::new(|| {
                arena_reset();
                via_widgets()
            })
        }
        other => {
            eprintln!("unknown mode {other}");
            std::process::exit(2);
        }
    };
    for _ in 0..WARMUP {
        std::hint::black_box(run());
    }
    let mut us: Vec<f64> = Vec::with_capacity(SAMPLES);
    let mut nodes = 0;
    for _ in 0..SAMPLES {
        let t = Instant::now();
        nodes = std::hint::black_box(run());
        us.push(t.elapsed().as_secs_f64() * 1e6);
    }
    us.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "{mode}\tmin={:.1}\tmedian={:.1}\tnodes={nodes}",
        us[0],
        us[SAMPLES / 2]
    );
}
