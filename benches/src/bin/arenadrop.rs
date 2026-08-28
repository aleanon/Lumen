//! Does a *production-viable* arena keep O0.17's win once it must track drops?
//!
//! O0.17 measured −16.6% for a bump-allocated widget tree, but by a device that
//! cannot ship: a `Box` over arena memory, with a custom global allocator
//! recognising arena addresses and skipping their free. Stable Rust has no
//! `allocator_api`, and `Box<Self>` is the only owning `self` type `dyn` will
//! take — so the honest question is what the *stable* mechanism costs.
//!
//! That mechanism is `&mut dyn DirectDyn` over `Option<W>` placed in a bump
//! region, with an explicit drop list: one `(ptr, drop_glue)` push per node,
//! walked in reverse at reset. Our widgets own `String`s and `Rc`s, so a bump
//! arena that skips destructors would leak a frame's worth of strings every
//! frame — drop tracking is mandatory, not optional, and its cost belongs in
//! the number.
//!
//! Arms, one process each: `element`, `boxed`, `arena` (drop-tracked, stable).

use lumen_widgets::direct::{lower_element, node, Column, DirectDyn, Node, TreeSink};
use lumen_widgets::{Button, Element, Label, ProgressBar};
use std::cell::{Cell, RefCell};
use std::time::Instant;

const ROWS: usize = 500;
const WARMUP: usize = 20;
const SAMPLES: usize = 100;

// --- a bump arena that runs destructors -------------------------------------

unsafe fn glue<T>(p: *mut u8) {
    unsafe { std::ptr::drop_in_place(p as *mut T) }
}

/// One entry in the arena's drop list: where a value lives, and how to drop it.
type DropEntry = (*mut u8, unsafe fn(*mut u8));

struct Arena {
    buf: RefCell<Vec<u8>>,
    off: Cell<usize>,
    drops: RefCell<Vec<DropEntry>>,
}

impl Arena {
    fn with_capacity(n: usize) -> Arena {
        Arena {
            buf: RefCell::new(vec![0u8; n]),
            off: Cell::new(0),
            drops: RefCell::new(Vec::with_capacity(4096)),
        }
    }

    /// Place `w` in the arena and hand back an erased reference to it.
    ///
    /// `&self`, not `&mut self`, so many live children can coexist — the
    /// bumpalo model. Sound because every allocation is a disjoint range of the
    /// buffer and the buffer never moves while references are out.
    ///
    /// No `'static` bound: a `Column` holds `&mut dyn DirectDyn`s borrowed from
    /// this same arena, so the tree is emphatically not `'static`. Soundness
    /// comes from ordering — every reference dies before `reset` runs the
    /// destructors — not from the lifetime bound.
    #[allow(clippy::mut_from_ref)]
    fn alloc<'a, W: lumen_widgets::direct::Direct + 'a>(
        &'a self,
        w: W,
    ) -> &'a mut (dyn DirectDyn + 'a) {
        let l = std::alloc::Layout::new::<Option<W>>();
        let base = self.buf.borrow().as_ptr() as usize;
        let off = (base + self.off.get() + l.align() - 1) & !(l.align() - 1);
        let end = off + l.size() - base;
        assert!(end < self.buf.borrow().len(), "arena exhausted");
        self.off.set(end);
        let p = off as *mut Option<W>;
        unsafe {
            std::ptr::write(p, Some(w));
            self.drops
                .borrow_mut()
                .push((p as *mut u8, glue::<Option<W>>));
            &mut *p
        }
    }

    /// Run every destructor, newest first, then rewind.
    fn reset(&self) {
        for (p, f) in self.drops.borrow_mut().drain(..).rev() {
            unsafe { f(p) }
        }
        self.off.set(0);
    }
}

// --- arms -------------------------------------------------------------------

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

fn via_element() -> (usize, u64) {
    let mut sink = TreeSink::new();
    lower_element(element_tree(), &mut sink, None);
    (sink.tree.len(), shape(&sink))
}

fn via_boxed() -> (usize, u64) {
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
    (sink.tree.len(), shape(&sink))
}

fn via_arena(a: &Arena) -> (usize, u64) {
    let n = {
        let rows: Vec<&mut dyn DirectDyn> = (0..ROWS)
            .map(|i| {
                let kids: Vec<&mut dyn DirectDyn> = vec![
                    a.alloc(Label::new(format!("row {i}")).size(14.0)),
                    a.alloc(ProgressBar::new(i as f64 / ROWS as f64)),
                    a.alloc(Button::new("Open").ghost().on_press(|_| {})),
                ];
                a.alloc(Column::new(kids).gap(8.0).padding(4.0))
            })
            .collect();
        let mut sink = TreeSink::new();
        a.alloc(Column::new(rows)).lower_dyn(&mut sink, None);
        (sink.tree.len(), shape(&sink))
    };
    a.reset();
    n
}

/// A widget that exists only to be dropped, so the arena's drop list can be
/// proved to run. Without this the `arena` arm would look *faster* precisely by
/// leaking — every widget owns a `String`, so a skipped destructor is both a
/// leak and an unearned win.
struct Tracer(#[allow(dead_code)] String);
static DROPPED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
impl Drop for Tracer {
    fn drop(&mut self) {
        DROPPED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}
impl lumen_widgets::direct::Direct for Tracer {
    fn lower_owned(
        self,
        _out: &mut TreeSink,
        _parent: Option<lumen_core::NodeIndex>,
    ) -> (lumen_core::NodeIndex, lumen_layout::LayoutNode) {
        unreachable!("the tracer is never lowered, only dropped")
    }
}

fn verify_drops() {
    use std::sync::atomic::Ordering;
    let a = Arena::with_capacity(1 << 20);
    const N: usize = 1000;
    {
        let _kids: Vec<&mut dyn DirectDyn> =
            (0..N).map(|i| a.alloc(Tracer(format!("s{i}")))).collect();
    }
    assert_eq!(
        DROPPED.load(Ordering::Relaxed),
        0,
        "nothing drops before reset"
    );
    a.reset();
    assert_eq!(
        DROPPED.load(Ordering::Relaxed),
        N,
        "every arena value's destructor must run at reset, or the arm is fast \
         because it leaks"
    );
    // And again, to prove reset leaves the arena reusable rather than merely
    // emptied once.
    {
        let _kids: Vec<&mut dyn DirectDyn> =
            (0..N).map(|i| a.alloc(Tracer(format!("t{i}")))).collect();
    }
    a.reset();
    assert_eq!(
        DROPPED.load(Ordering::Relaxed),
        2 * N,
        "reset is repeatable"
    );
    println!("drop check: {} destructors ran", 2 * N);
}

/// The shape this benchmark exists to test: children as **statements**, not
/// values. The sink is handed to the builder and each widget is written
/// straight into the SoA — no `Element`, no `Box`, no arena, no trait object.
/// Heterogeneity comes from control flow rather than from type erasure, which
/// is what the `Vec<Box<dyn ..>>` form was buying.
fn via_inline() -> (usize, u64) {
    use lumen_core::semantics::Role;
    let mut sink = TreeSink::new();
    let mut root = sink.node(None, Role::Group).elide(true).resolve();
    let rn = root.index();
    let mut row_lns = Vec::with_capacity(ROWS);
    for i in 0..ROWS {
        let mut row = root.begin_child(Role::Group).elide(true).resolve();
        let n = row.index();
        let a = row.child_of(Label::new(format!("row {i}")).size(14.0));
        let b = row.child_of(ProgressBar::new(i as f64 / ROWS as f64));
        let c = row.child_of(Button::new("Open").ghost().on_press(|_| {}));
        let _ = n;
        row_lns.push(row.end(&col_style(8.0, 4.0), &[a, b, c], false));
    }
    let _ = rn;
    // `LayoutStyle::default()` is a flex ROW; `Column` closes with a column.
    // Using the default here would have made this arm lay out a different tree
    // from the others and quietly won the comparison on layout, not on
    // representation.
    root.end(&col_style(0.0, 0.0), &row_lns, false);
    (sink.tree.len(), shape(&sink))
}

/// The same layout style `Column` closes with, so the arms are comparable.
fn col_style(gap: f32, padding: f32) -> lumen_layout::LayoutStyle {
    lumen_layout::LayoutStyle {
        display: lumen_layout::Display::Flex,
        flex_direction: lumen_layout::FlexDirection::Column,
        row_gap: lumen_layout::Dim::px(gap),
        padding: lumen_layout::Edges::all(lumen_layout::Dim::px(padding)),
        ..Default::default()
    }
}

/// Fingerprint the produced tree, so the arms are known to build the same
/// thing rather than assumed to. Preorder over (role, depth) — structure and
/// node kind, which is what differs if an arm quietly builds a cheaper tree.
fn shape(sink: &TreeSink) -> u64 {
    fn walk(sink: &TreeSink, n: lumen_core::NodeIndex, d: u64, h: &mut u64) {
        *h = h
            .wrapping_mul(1099511628211)
            .wrapping_add(sink.meta.role(n) as u64)
            .wrapping_mul(1099511628211)
            .wrapping_add(d);
        let mut c = sink.tree.first_child(n);
        while c.is_some() {
            walk(sink, c, d + 1, h);
            c = sink.tree.next_sibling(c);
        }
    }
    let mut h = 1469598103934665603u64;
    walk(sink, sink.tree.root(), 0, &mut h);
    h
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "element".into());
    if mode == "verify" {
        verify_drops();
        return;
    }
    verify_drops();
    let arena = Arena::with_capacity(8 << 20);
    let run: Box<dyn Fn() -> (usize, u64)> = match mode.as_str() {
        "element" => Box::new(via_element),
        "boxed" => Box::new(via_boxed),
        "arena" => Box::new(|| via_arena(&arena)),
        "inline" => Box::new(via_inline),
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
    let mut fp = 0u64;
    for _ in 0..SAMPLES {
        let t = Instant::now();
        let r = std::hint::black_box(run());
        nodes = r.0;
        fp = r.1;
        us.push(t.elapsed().as_secs_f64() * 1e6);
    }
    us.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "{mode}\tmin={:.1}\tmedian={:.1}\tnodes={nodes}\tshape={fp:016x}",
        us[0],
        us[SAMPLES / 2]
    );
}
