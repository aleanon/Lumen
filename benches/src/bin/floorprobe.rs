//! Where does the per-node cost sit once `Element` is gone?
//!
//! Direct lowering leaves ~2.7 allocations per node. This attributes them, by
//! building the same node count with progressively less content: a bare group
//! (tree + taffy + an empty side-table record), then one with an id, then a
//! full widget. The gaps are the answer, and they say what to attack next.

use lumen_core::semantics::Role;
use lumen_layout::LayoutStyle;
use lumen_widgets::direct::{Direct, TreeSink};
use lumen_widgets::Label;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

struct C;
unsafe impl GlobalAlloc for C {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(l.size(), Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(n.saturating_sub(l.size()), Ordering::Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static A: C = C;

const N: usize = 2000;

fn cost(f: impl FnOnce()) -> (usize, usize) {
    let (a, b) = (
        ALLOCS.load(Ordering::Relaxed),
        BYTES.load(Ordering::Relaxed),
    );
    f();
    (
        ALLOCS.load(Ordering::Relaxed) - a,
        BYTES.load(Ordering::Relaxed) - b,
    )
}

fn main() {
    // Warm the arena.
    {
        let mut s = TreeSink::new();
        let r = s.node(None, Role::Group).resolve();
        r.end(&LayoutStyle::default(), &[], false);
    }

    // 1. The floor: a bare node. Tree slot + taffy node + side-table record.
    let (bare_a, bare_b) = cost(|| {
        let mut s = TreeSink::new();
        let mut root = s.node(None, Role::Group).resolve();
        let rn = root.index();
        let mut lns = Vec::with_capacity(N);
        for _ in 0..N {
            let c = root.sink().node(Some(rn), Role::Group).resolve();
            lns.push(c.end(&LayoutStyle::default(), &[], false));
        }
        root.end(&LayoutStyle::default(), &lns, false);
        std::hint::black_box(s.tree.len());
    });

    // 2. …plus a stable id, which every addressable node carries.
    let (id_a, id_b) = cost(|| {
        let mut s = TreeSink::new();
        let mut root = s.node(None, Role::Group).resolve();
        let rn = root.index();
        let mut lns = Vec::with_capacity(N);
        for i in 0..N {
            let c = root
                .sink()
                .node(Some(rn), Role::Group)
                .id(format!("n{i}"))
                .resolve();
            lns.push(c.end(&LayoutStyle::default(), &[], false));
        }
        root.end(&LayoutStyle::default(), &lns, false);
        std::hint::black_box(s.tree.len());
    });

    // 3. …plus a class, the other matchable string.
    let (cls_a, cls_b) = cost(|| {
        let mut s = TreeSink::new();
        let mut root = s.node(None, Role::Group).resolve();
        let rn = root.index();
        let mut lns = Vec::with_capacity(N);
        for i in 0..N {
            let c = root
                .sink()
                .node(Some(rn), Role::Group)
                .id(format!("n{i}"))
                .class("row")
                .resolve();
            lns.push(c.end(&LayoutStyle::default(), &[], false));
        }
        root.end(&LayoutStyle::default(), &lns, false);
        std::hint::black_box(s.tree.len());
    });

    // 4. A real text widget: label string + content + measurement.
    let (lbl_a, lbl_b) = cost(|| {
        let mut s = TreeSink::new();
        let mut root = s.node(None, Role::Group).resolve();
        let rn = root.index();
        let mut lns = Vec::with_capacity(N);
        for i in 0..N {
            let (_, ln) = Label::new(format!("row {i}")).lower(root.sink(), Some(rn));
            lns.push(ln);
        }
        root.end(&LayoutStyle::default(), &lns, false);
        std::hint::black_box(s.tree.len());
    });

    let per = |a: usize| a as f64 / N as f64;
    println!("\nPer-node cost after Element is removed ({N} nodes)");
    println!("──────────────────────────────────────────────────────────────");
    println!("  {:<26}{:>9}{:>12}{:>10}", "", "allocs", "allocs/node", "KiB");
    println!("  {:<26}{:>9}{:>12.2}{:>10}", "bare node (floor)", bare_a, per(bare_a), bare_b / 1024);
    println!("  {:<26}{:>9}{:>12.2}{:>10}", "+ stable id", id_a, per(id_a), id_b / 1024);
    println!("  {:<26}{:>9}{:>12.2}{:>10}", "+ one class", cls_a, per(cls_a), cls_b / 1024);
    println!("  {:<26}{:>9}{:>12.2}{:>10}", "full Label widget", lbl_a, per(lbl_a), lbl_b / 1024);
    println!("──────────────────────────────────────────────────────────────");
    println!("  attribution per node:");
    println!("    tree slot + taffy + record : {:>5.2} allocs", per(bare_a));
    println!("    the id string              : {:>5.2}", per(id_a) - per(bare_a));
    println!("    the class string + Vec     : {:>5.2}", per(cls_a) - per(id_a));
    println!("\n  size_of::<Meta>()           : {:>5} B", std::mem::size_of::<lumen_widgets::direct::Meta>());
    println!();
}
