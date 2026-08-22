//! What does a mutation actually cost, by pump tier?
//!
//! Same 3000-row list every time; only HOW row 0 changes differs. This is the
//! measurement behind "why re-run build at all — why not build once and
//! mutate?": Lumen already has the mutate path, and this prices it against the
//! rebuild path on identical content.
use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{bind, widgets, App};
use std::time::Instant;

const N: usize = 3000;

struct NullRenderer;
impl lumen_render::Renderer for NullRenderer {
    fn render_frame(&mut self, _l: &lumen_render::DisplayList, _w: u32, _h: u32, _s: f64,
                    _b: lumen_core::Color) -> lumen_render::RgbaImage {
        lumen_render::RgbaImage::from_raw(0, 0, Vec::new())
    }
    fn name(&self) -> &'static str { "null" }
}

fn bench(label: &str, mut h: lumen_widgets::Headless<NullRenderer>, iters: u32) {
    h.pump();
    let sig: Signal<i64> = h.runtime().signal("n", || 0);
    // warm
    for _ in 0..20 { sig.update(h.runtime(), |v| *v += 1); h.pump(); }
    let mut best = f64::MAX;
    for _ in 0..iters {
        sig.update(h.runtime(), |v| *v += 1);
        let t = Instant::now();
        let st = h.pump();
        let us = t.elapsed().as_secs_f64() * 1e6;
        if us < best { best = us; }
        std::hint::black_box(st);
    }
    println!("{label:<44} {best:>9.1} us");
}

fn main() {
    // 1. IDLE: nothing changed at all.
    {
        let mut h = App::new(move |_cx| {
            widgets::column((0..N).map(|i| widgets::text(format!("row {i}"))).collect::<Vec<_>>())
        })
        .with_renderer(NullRenderer)
        .run_headless(Size::new(400.0, 800.0));
        h.pump();
        let mut best = f64::MAX;
        for _ in 0..200 {
            let t = Instant::now();
            std::hint::black_box(h.pump());
            let us = t.elapsed().as_secs_f64() * 1e6;
            if us < best { best = us; }
        }
        println!("{:<44} {best:>9.1} us", "idle pump (nothing changed)");
    }

    // 2. PATCH TIER: row 0's BACKGROUND is a paint-only binding.
    bench("one row's background (paint-only binding)", App::new(move |cx| {
        let _ = cx.signal("n", || 0i64);
        let rows: Vec<_> = (0..N).map(|i| {
            let t = widgets::text(format!("row {i}"));
            if i == 0 {
                t.bind_background(bind!(rt => {
                    let s: Signal<i64> = rt.signal("n", || 0i64);
                    let v = s.get(rt) as u8;
                    lumen_core::Color::srgb8(v, 0, 0, 255)
                }))
            } else { t }
        }).collect();
        widgets::column(rows)
    })
    .with_renderer(NullRenderer)
    .run_headless(Size::new(400.0, 800.0)), 200);

    // 3. PATCH TIER, TEXT (F3.5): row 0's text is a binding whose value keeps
    //    the same measured size, so the pump patches instead of rebuilding.
    bench("one row's text, bound + same size (patch)", App::new(move |cx| {
        let _ = cx.signal("n", || 0i64);
        let rows: Vec<_> = (0..N).map(|i| {
            let t = widgets::text(format!("row {i}"));
            if i == 0 {
                t.bind_text(bind!(rt => {
                    let s: Signal<i64> = rt.signal("n", || 0i64);
                    format!("counter: {:04}", s.get(rt) % 10000)
                }))
            } else { t }
        }).collect();
        widgets::column(rows)
    })
    .with_renderer(NullRenderer)
    .run_headless(Size::new(400.0, 800.0)), 200);

    // 3b. F3.6: a bound label inside a memoized span, forced through the
    //     REBUILD path by a structural change. Before F3.6 the binding made
    //     the span impure, so nothing spliced and this was a full rebuild.
    {
        let mut h = App::new(move |cx| {
            let rows: Signal<usize> = cx.signal("rows", || N);
            let n: Signal<i64> = cx.signal("n", || 0);
            let count = rows.get(cx.runtime());
            let kids: Vec<_> = (0..count).map(|i| {
                cx.scope(("row", i), move |_cx| {
                    // EVERY row bound — the shape step 2 would produce if all
                    // text became a binding. One bound row proves nothing: the
                    // old `impure` rule only bit the spans that carried a
                    // binding, so a single one cost a single re-lowered row.
                    widgets::text(format!("row {i}")).bind_text(bind!(rt => {
                        let s: Signal<i64> = rt.signal("n", || 0i64);
                        format!("row {i}: {:04}", s.get(rt) % 10000)
                    }))
                })
            }).collect();
            widgets::column(kids)
        })
        .with_renderer(NullRenderer)
        .run_headless(Size::new(400.0, 800.0));
        h.pump();
        let rows: Signal<usize> = h.runtime().signal("rows", || N);
        for k in 0..20 { rows.set(h.runtime(), N - (k % 2)); h.pump(); }
        let mut best = f64::MAX;
        for k in 0..100 {
            rows.set(h.runtime(), N - (k % 2));
            let t = Instant::now();
            let st = h.pump();
            let us = t.elapsed().as_secs_f64() * 1e6;
            if us < best { best = us; }
            std::hint::black_box(st);
        }
        println!("{:<44} {best:>9.1} us", "structural change, ALL rows bound (rebuild)");
    }

    // 4. REBUILD TIER: row 0's TEXT changes, memoized per row (today's bench).
    bench("one row's text, memoized per row (rebuild)", App::new(move |cx| {
        let bump = cx.signal("n", || 0i64).get(cx.runtime());
        let rows: Vec<_> = (0..N).map(|i| {
            let dep = if i == 0 { bump } else { 0 };
            cx.scope_with_deps(("row", i), dep, move |_cx| {
                if i == 0 { widgets::text(format!("counter: {bump}")) }
                else { widgets::text(format!("row {i}")) }
            })
        }).collect();
        widgets::column(rows)
    })
    .with_renderer(NullRenderer)
    .run_headless(Size::new(400.0, 800.0)), 100);
}
