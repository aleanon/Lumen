//! WT-EXP — the *resource* half of the Widget-trait experiment.
//!
//! Timing lives in `benches/widgetcost.rs`; this binary answers the questions
//! criterion cannot:
//!
//!   1. **Sizes.** How many bytes is a widget value? Under eager lowering every
//!      typed widget is a newtype over a whole `Element`, so they are all
//!      exactly `size_of::<Element>()`. That is the number the experiment is
//!      trying to move.
//!   2. **Allocations.** Counted with the same counting-allocator shape
//!      `nodecost.rs` uses, for widget construction *and* for a changed frame.
//!   3. **Peak RSS.** Read from `/proc/self/statm` after holding a large tree
//!      live, because "resource usage" is not only allocation count.
//!
//! Deterministic and single-shot: run it in both trees and diff the output.

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{
    widgets, App, Button, Card, CheckBox, Chip, Container, Element, Label, ProgressBar,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

// --- counting allocator (same shape as benches/nodecost.rs) -----------------

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(l.size(), Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(new.saturating_sub(l.size()), Ordering::Relaxed);
        unsafe { System.realloc(p, l, new) }
    }
}

#[global_allocator]
static A: Counting = Counting;

/// (allocations, bytes) performed while running `f`.
fn cost_of<T>(f: impl FnOnce() -> T) -> (T, usize, usize) {
    let a0 = ALLOCS.load(Ordering::Relaxed);
    let b0 = BYTES.load(Ordering::Relaxed);
    let out = f();
    (
        out,
        ALLOCS.load(Ordering::Relaxed) - a0,
        BYTES.load(Ordering::Relaxed) - b0,
    )
}

/// Resident set size in KiB, straight from the kernel.
fn rss_kib() -> usize {
    let s = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
    let pages: usize = s.split_whitespace().nth(1).unwrap_or("0").parse().unwrap_or(0);
    pages * 4096 / 1024
}

const N: usize = 1_000;

fn main() {
    // --- 1. sizes ----------------------------------------------------------
    println!("\nWT-EXP — widget value sizes (bytes)");
    println!("──────────────────────────────────────────────");
    println!("  Element        : {:>6}", std::mem::size_of::<Element>());
    println!("  LayoutStyle    : {:>6}", std::mem::size_of::<lumen_layout::LayoutStyle>());
    println!("  ---");
    println!("  Button         : {:>6}", std::mem::size_of::<Button>());
    println!("  Label          : {:>6}", std::mem::size_of::<Label>());
    println!("  Container      : {:>6}", std::mem::size_of::<Container>());
    println!("  Card           : {:>6}", std::mem::size_of::<Card>());
    println!("  Chip           : {:>6}", std::mem::size_of::<Chip>());
    println!("  CheckBox       : {:>6}", std::mem::size_of::<CheckBox>());
    println!("  ProgressBar    : {:>6}", std::mem::size_of::<ProgressBar>());

    // --- 2. allocations, pure construction ---------------------------------
    // Warm the allocator arena first so first-touch growth isn't attributed.
    {
        let v: Vec<Element> = (0..N).map(|_| Button::new("Save").into()).collect();
        std::hint::black_box(v.len());
    }

    let (_, btn_a, btn_b) = cost_of(|| {
        let v: Vec<Element> = (0..N)
            .map(|i| {
                Button::new("Save")
                    .ghost()
                    .on_press(|_| {})
                    .id(format!("btn{i}"))
                    .into()
            })
            .collect();
        v.len()
    });

    let (_, lbl_a, lbl_b) = cost_of(|| {
        let v: Vec<Element> = (0..N)
            .map(|i| {
                Label::new(format!("row {i}"))
                    .size(15.0)
                    .bold()
                    .color(lumen_core::Color::WHITE)
                    .into()
            })
            .collect();
        v.len()
    });

    let (_, mix_a, mix_b) = cost_of(|| {
        let v: Vec<Element> = (0..N)
            .map(|i| {
                Container::new(vec![
                    Label::new(format!("item {i}")).size(14.0).into(),
                    Chip::new("new").into(),
                    Button::new("Open").on_press(|_| {}).into(),
                ])
                .row()
                .gap(8.0)
                .into()
            })
            .collect();
        v.len()
    });

    let (_, card_a, card_b) = cost_of(|| {
        let v: Vec<Element> = (0..N)
            .map(|_| Card::new(vec![widgets::text("body")]).into())
            .collect();
        v.len()
    });

    let (_, prog_a, prog_b) = cost_of(|| {
        let v: Vec<Element> = (0..N)
            .map(|i| ProgressBar::new(i as f64 / N as f64).into())
            .collect();
        v.len()
    });

    println!("\nWT-EXP — allocations to build {N} widgets");
    println!("──────────────────────────────────────────────────────────────");
    println!("  button_1k   : {btn_a:>7} allocs  {:>7} KiB   ({:.1}/widget)", btn_b / 1024, btn_a as f64 / N as f64);
    println!("  label_1k    : {lbl_a:>7} allocs  {:>7} KiB   ({:.1}/widget)", lbl_b / 1024, lbl_a as f64 / N as f64);
    println!("  card_1k     : {card_a:>7} allocs  {:>7} KiB   ({:.1}/widget)", card_b / 1024, card_a as f64 / N as f64);
    println!("  progress_1k : {prog_a:>7} allocs  {:>7} KiB   ({:.1}/widget)", prog_b / 1024, prog_a as f64 / N as f64);
    println!("  mixed_1k    : {mix_a:>7} allocs  {:>7} KiB   ({:.1}/row)", mix_b / 1024, mix_a as f64 / N as f64);

    // --- 3. allocations + RSS for a real frame ------------------------------
    let rss_before = rss_kib();
    let mut h = widget_app(500).run_headless(Size::new(600.0, 800.0));
    for _ in 0..5 {
        h.pump();
    }
    let rss_live = rss_kib();

    // One warm changed frame, then the measured one.
    {
        let s: Signal<i64> = h.runtime().signal("n", || 0);
        s.update(h.runtime(), |v| *v += 1);
        h.pump();
    }
    let (_, frame_a, frame_b) = cost_of(|| {
        let s: Signal<i64> = h.runtime().signal("n", || 0);
        s.update(h.runtime(), |v| *v += 1);
        h.pump();
    });
    let (_, idle_a, idle_b) = cost_of(|| h.pump());
    let rss_after = rss_kib();

    println!("\nWT-EXP — 500 typed-widget rows (1500 widgets, ~3500 nodes)");
    println!("──────────────────────────────────────────────────────────────");
    println!("  changed frame : {frame_a:>7} allocs  {:>7} KiB", frame_b / 1024);
    println!("  idle pump     : {idle_a:>7} allocs  {:>7} KiB", idle_b / 1024);
    println!("  RSS at start  : {rss_before:>7} KiB");
    println!("  RSS with app  : {rss_live:>7} KiB   (+{} KiB)", rss_live.saturating_sub(rss_before));
    println!("  RSS at end    : {rss_after:>7} KiB   (+{} KiB)", rss_after.saturating_sub(rss_before));
    println!();

    std::hint::black_box(&h);
}

/// Same shape as `benches/widgetcost.rs::widget_app`, kept identical so the two
/// instruments describe the same workload.
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
