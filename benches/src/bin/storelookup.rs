//! What does the keyed signal store cost, against a plain field read?
//!
//! The question behind a `#[derive(Reactive)]` state struct: today a view reads
//! per-row state as `cx.signal(key).get(rt)`. A state struct would read a
//! field. R7 attributed ~2.7 ms of a 9.0 ms frame (N=50 000) to this, but that
//! was inferred from the `view` phase rather than measured directly.
//!
//! Three layers, because they are eliminated by *different* things:
//!
//!   address+read   `rt.signal(k, ..).get(rt)`  — what a view does today
//!   read only      `handle.get(rt)`            — handle already resolved
//!   field read     `vals[i]`                   — the floor
//!
//! `address+read − read only` is the **addressing** cost: hashing the key and
//! resolving it to a `SignalId`. That is what a compile-time field path
//! removes.
//!
//! `read only − field read` is slot lookup + downcast + read recording. A state
//! struct removes the first two; **read recording stays**, because the engine
//! must still know which fields a component read. Reporting these separately is
//! the point — quoting only the total would overstate what the change buys.
//!
//! Measured with a read collector open and closed: recording only happens
//! inside a build, so the closed number alone would understate the real cost.

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App, BuildCx};
use std::time::Instant;

fn env(k: &str, d: usize) -> usize {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
}

fn best_of<F: FnMut() -> i64>(rounds: usize, mut f: F) -> (f64, i64) {
    let mut sink = 0i64;
    for _ in 0..3 {
        sink = sink.wrapping_add(f());
    }
    let mut best = f64::MAX;
    for _ in 0..rounds {
        let t = Instant::now();
        let v = f();
        let us = t.elapsed().as_secs_f64() * 1e6;
        sink = sink.wrapping_add(v);
        best = best.min(us);
    }
    (best, sink)
}

fn main() {
    let n = env("N", 50_000);
    let rounds = env("ROUNDS", 40);

    let mut h = App::new(|_cx: &mut BuildCx| widgets::text("x"))
        .run_headless(Size::new(100.0, 40.0));
    h.pump();
    let rt = h.runtime();

    // Populate n signals and keep their handles.
    let handles: Vec<Signal<i64>> = (0..n).map(|i| rt.signal(i, || i as i64)).collect();
    let vals: Vec<i64> = (0..n).map(|i| i as i64).collect();

    // --- collector CLOSED (no read recording) -------------------------------
    let (addr_read, s1) = best_of(rounds, || {
        let mut acc = 0i64;
        for i in 0..n {
            let s: Signal<i64> = rt.signal(i, || 0);
            acc = acc.wrapping_add(s.get(rt));
        }
        acc
    });
    let (read_only, s2) = best_of(rounds, || {
        let mut acc = 0i64;
        for h in &handles {
            acc = acc.wrapping_add(h.get(rt));
        }
        acc
    });
    let (field, s3) = best_of(rounds, || {
        let mut acc = 0i64;
        for v in &vals {
            acc = acc.wrapping_add(*v);
        }
        acc
    });

    // --- collector OPEN (reads are recorded, as in a real build) ------------
    let (addr_read_c, s4) = best_of(rounds, || {
        let (acc, _reads) = rt.collect_reads(|| {
            let mut acc = 0i64;
            for i in 0..n {
                let s: Signal<i64> = rt.signal(i, || 0);
                acc = acc.wrapping_add(s.get(rt));
            }
            acc
        });
        acc
    });
    let (read_only_c, s5) = best_of(rounds, || {
        let (acc, _reads) = rt.collect_reads(|| {
            let mut acc = 0i64;
            for h in &handles {
                acc = acc.wrapping_add(h.get(rt));
            }
            acc
        });
        acc
    });

    // Equivalence: every arm must sum the same values, or one of them is not
    // doing the work.
    let expect: i64 = (0..n as i64).sum();
    for (name, got) in [
        ("addr+read", s1),
        ("read-only", s2),
        ("field", s3),
        ("addr+read/collect", s4),
        ("read-only/collect", s5),
    ] {
        let per = got / (rounds as i64 + 3);
        assert_eq!(
            per, expect,
            "{name} summed {per}, expected {expect} — that arm is not reading \
             the same data as the others"
        );
    }

    let us = |x: f64| x;
    let per_ns = |x: f64| x * 1000.0 / n as f64;
    println!("N={n}  (µs total, ns/read)");
    println!("  collector CLOSED");
    println!("    address+read   {:8.1}  {:6.1} ns", us(addr_read), per_ns(addr_read));
    println!("    read only      {:8.1}  {:6.1} ns", us(read_only), per_ns(read_only));
    println!("    field read     {:8.1}  {:6.1} ns", us(field), per_ns(field));
    println!("  collector OPEN (as in a build)");
    println!("    address+read   {:8.1}  {:6.1} ns", us(addr_read_c), per_ns(addr_read_c));
    println!("    read only      {:8.1}  {:6.1} ns", us(read_only_c), per_ns(read_only_c));
    println!("  ----");
    println!(
        "    addressing (removed by field paths) {:8.1} µs",
        addr_read_c - read_only_c
    );
    println!(
        "    lookup+downcast+record (partly stays) {:8.1} µs",
        read_only_c - field
    );
}
