//! PROF2 — sampling-profiler harness for the per-node floor.
//!
//! PROF1 (`docs/profile-vs-iced-2026-08-19.md`) priced the floor by subtraction
//! and left 45% of it unattributed, because `perf_event_paranoid` was 4 on the
//! measuring box. This binary is the instrument that closes that: it runs ONE
//! phase variant in a long loop, so a sampling profiler gets enough samples to
//! name the functions instead of inferring them.
//!
//! `probe_phases` runs each variant for ~60 frames (~90 ms) — fine for a
//! stopwatch, far too short for a 3 kHz sampler. Same views, same 3000 rows,
//! same NullRenderer; only the duration differs.
//!
//! ```sh
//! cargo build --release --bin probe_perf
//! perf record -F 3000 --call-graph dwarf -o boxes.data -- \
//!     ../target/release/probe_perf boxes 12
//! perf report -i boxes.data --stdio --no-children
//! ```
//!
//! Variants match PROF1's Finding 3: `text` = A (baseline), `sized` = B (text
//! shaped but not measured by layout), `boxes` = C (the content-free floor —
//! the one whose 45% was unattributed).
use kurbo::Size;
use lumen_core::state::Signal;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, BuildCx, Element};
use std::time::{Duration, Instant};

const N: usize = 3000;

struct NullRenderer;
impl lumen_render::Renderer for NullRenderer {
    fn render_frame(
        &mut self,
        _l: &lumen_render::DisplayList,
        _w: u32,
        _h: u32,
        _s: f64,
        _b: lumen_core::Color,
    ) -> lumen_render::RgbaImage {
        lumen_render::RgbaImage::from_raw(0, 0, Vec::new())
    }
    fn name(&self) -> &'static str {
        "null"
    }
}

/// A: the benchmark baseline — text rows, layout measures each one.
fn view_text(cx: &mut BuildCx) -> Element {
    let bump = cx.signal("n", || 0i64).get(cx.runtime());
    widgets::column(
        (0..N)
            .map(|i| {
                if i == 0 {
                    widgets::text(format!("counter: {bump}"))
                } else {
                    widgets::text(format!("row {i}"))
                }
            })
            .collect::<Vec<_>>(),
    )
}

/// B: text still shaped for paint, but layout no longer has to measure it.
fn view_sized(cx: &mut BuildCx) -> Element {
    let bump = cx.signal("n", || 0i64).get(cx.runtime());
    widgets::column(
        (0..N)
            .map(|i| {
                let mut e = if i == 0 {
                    widgets::text(format!("counter: {bump}"))
                } else {
                    widgets::text(format!("row {i}"))
                };
                e.style.width = Dim::px(400.0);
                e.style.height = Dim::px(16.0);
                e
            })
            .collect::<Vec<_>>(),
    )
}

/// C: no text at all — the per-node pipeline alone (element → arena → taffy →
/// paint). This is the 0.516 us/node floor PROF1 could only half account for.
fn view_boxes(cx: &mut BuildCx) -> Element {
    // Matches probe_phases variant C exactly (`Element::column`, not
    // `widgets::column`, and a uniform 16px height) so the number here is
    // comparable to PROF1's 0.516 us/row floor.
    let bump = cx.signal("n", || 0i64).get(cx.runtime());
    let _ = bump;
    widgets::column(
        (0..N)
            .map(|_| {
                let mut e = Element::column(Vec::new());
                e.style.width = Dim::px(400.0);
                e.style.height = Dim::px(16.0);
                e
            })
            .collect::<Vec<_>>(),
    )
}

fn run(build: impl Fn(&mut BuildCx) -> Element + 'static, secs: u64) {
    let mut h = App::new(build)
        .with_renderer(NullRenderer)
        .run_headless(Size::new(400.0, 800.0));
    h.pump();
    let sig: Signal<i64> = h.runtime().signal("n", || 0);
    for _ in 0..5 {
        sig.update(h.runtime(), |v| *v += 1);
        h.pump();
    }
    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut frames = 0u64;
    let t0 = Instant::now();
    while Instant::now() < deadline {
        sig.update(h.runtime(), |v| *v += 1);
        h.pump();
        frames += 1;
    }
    let us = t0.elapsed().as_secs_f64() * 1e6 / frames as f64;
    eprintln!(
        "{frames} frames  {us:.1} us/frame  {:.3} us/row",
        us / N as f64
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let variant = args.next().unwrap_or_else(|| "boxes".into());
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(10);
    eprintln!("variant={variant} secs={secs} rows={N}");
    match variant.as_str() {
        "text" => run(view_text, secs),
        "sized" => run(view_sized, secs),
        "boxes" => run(view_boxes, secs),
        other => {
            eprintln!("unknown variant {other}; use text|sized|boxes");
            std::process::exit(2);
        }
    }
}
