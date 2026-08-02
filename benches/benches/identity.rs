//! ADR-021 H3 — what hash identity actually bought.
//!
//! The claim behind the ADR is that per-item reactive state was expensive
//! because of the **string key that addresses a signal**, not the signal cell.
//! This measures it directly on the shape that motivated the work: a 1 000-row
//! list whose per-row state is re-addressed every frame.
//!
//! Two dimensions:
//!   * `id_addressing/*` — the cost of *addressing* 1 000 per-row signals, the
//!     old way (`format!("row-{i}")` per row per frame) vs a typed key.
//!   * `id_alloc/*` — the same, counted in **allocations** rather than time,
//!     via a counting global allocator. Time is noisy; allocation count is the
//!     thing the ADR actually changed, so it is asserted, not just reported.
//!
//! Run: `cargo bench -p lumen-benches --bench identity`

use criterion::{criterion_group, criterion_main, Criterion};
use lumen_core::state::{Runtime, Signal};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

// --- counting allocator -----------------------------------------------------

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(p, l, new) }
    }
}

#[global_allocator]
static A: Counting = Counting;

/// Allocations performed while running `f`.
fn allocs_during(f: impl FnOnce()) -> usize {
    let before = ALLOCS.load(Ordering::Relaxed);
    f();
    ALLOCS.load(Ordering::Relaxed) - before
}

const ROWS: u32 = 1_000;

/// A typed per-row key — no allocation to construct or to address with.
#[derive(Hash, Debug, Clone, Copy)]
enum Row {
    Hits(u32),
}

/// The pre-ADR-021 shape: build a fresh key string for every row, every frame.
fn address_all_by_string(rt: &Runtime) {
    for i in 0..ROWS {
        let s: Signal<u32> = rt.signal(format!("row-{i}"), || 0);
        std::hint::black_box(s);
    }
}

/// The ADR-021 shape: a `Copy` typed key.
fn address_all_by_typed_key(rt: &Runtime) {
    for i in 0..ROWS {
        let s: Signal<u32> = rt.signal(Row::Hits(i), || 0);
        std::hint::black_box(s);
    }
}

fn id_addressing(c: &mut Criterion) {
    // Warm both key spaces so every iteration measures *re-addressing* (the
    // per-frame steady state), not first-time interning.
    let rt = Runtime::new();
    address_all_by_string(&rt);
    address_all_by_typed_key(&rt);

    let mut g = c.benchmark_group("id_addressing");
    g.bench_function("1k_rows_string_key", |b| {
        b.iter(|| address_all_by_string(&rt))
    });
    g.bench_function("1k_rows_typed_key", |b| {
        b.iter(|| address_all_by_typed_key(&rt))
    });
    g.finish();
}

/// The headline number: allocations per frame to re-address 1 000 signals.
///
/// A typed key must be **zero**; a `format!` key is one allocation per row.
/// Asserted, so a regression that reintroduces per-frame key building fails the
/// bench rather than quietly costing 1 000 allocations a frame.
fn id_alloc(c: &mut Criterion) {
    let rt = Runtime::new();
    address_all_by_string(&rt);
    address_all_by_typed_key(&rt);

    let by_string = allocs_during(|| address_all_by_string(&rt));
    let by_typed = allocs_during(|| address_all_by_typed_key(&rt));

    println!(
        "\nADR-021 — re-addressing {ROWS} per-row signals for one frame:\n  \
         string key `format!(\"row-{{i}}\")` : {by_string} allocations\n  \
         typed key  `Row::Hits(i)`        : {by_typed} allocations\n"
    );

    assert_eq!(
        by_typed, 0,
        "a typed key must re-address with zero allocations — that is the whole \
         point of ADR-021; got {by_typed}"
    );
    assert!(
        by_string >= ROWS as usize,
        "expected the string-key path to allocate at least once per row, got {by_string}"
    );

    let mut g = c.benchmark_group("id_alloc");
    g.bench_function("1k_rows_typed_key_allocs", |b| {
        b.iter(|| std::hint::black_box(allocs_during(|| address_all_by_typed_key(&rt))))
    });
    g.finish();
}

criterion_group!(benches, id_addressing, id_alloc);
criterion_main!(benches);
