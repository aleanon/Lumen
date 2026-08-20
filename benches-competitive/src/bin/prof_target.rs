//! A long-running steady-state frame loop for `perf record`.
//!
//! `prof_target <variant> <seconds>` where variant is:
//!   text   — the benchmark's own view (text rows)
//!   empty  — empty fixed-size boxes, no text: the 0.516 us/node floor whose
//!            45% PROF1 could not attribute
//!   memo   — text rows with cx.scope_with_deps per row (the copy path)
use kurbo::Size;
use lumen_core::state::Signal;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, BuildCx, Element};
use std::time::{Duration, Instant};

const N: usize = 3000;

struct NullRenderer;
impl lumen_render::Renderer for NullRenderer {
    fn render_frame(&mut self, _l: &lumen_render::DisplayList, _w: u32, _h: u32, _s: f64,
                    _b: lumen_core::Color) -> lumen_render::RgbaImage {
        lumen_render::RgbaImage::from_raw(0, 0, Vec::new())
    }
    fn name(&self) -> &'static str { "null" }
}

fn run(build: impl Fn(&mut BuildCx) -> Element + 'static, secs: u64) {
    let mut h = App::new(build)
        .with_renderer(NullRenderer)
        .run_headless(Size::new(400.0, 800.0));
    h.pump();
    let sig: Signal<i64> = h.runtime().signal("n", || 0);
    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut frames = 0u64;
    while Instant::now() < deadline {
        sig.update(h.runtime(), |v| *v += 1);
        h.pump();
        frames += 1;
    }
    eprintln!("{frames} frames");
}

fn main() {
    let variant = std::env::args().nth(1).unwrap_or_else(|| "empty".into());
    let secs: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(10);
    match variant.as_str() {
        "text" => run(|cx| {
            let bump = cx.signal("n", || 0i64).get(cx.runtime());
            widgets::column((0..N).map(|i| if i == 0 {
                widgets::text(format!("counter: {bump}"))
            } else { widgets::text(format!("row {i}")) }).collect::<Vec<_>>())
        }, secs),
        "memo" => run(|cx| {
            let bump = cx.signal("n", || 0i64).get(cx.runtime());
            widgets::column((0..N).map(|i| {
                let dep = if i == 0 { bump } else { 0 };
                cx.scope_with_deps(("row", i), dep, move |_cx| {
                    if i == 0 { widgets::text(format!("counter: {bump}")) }
                    else { widgets::text(format!("row {i}")) }
                })
            }).collect::<Vec<_>>())
        }, secs),
        _ => run(|cx| {
            let bump = cx.signal("n", || 0i64).get(cx.runtime());
            widgets::column((0..N).map(|i| {
                let mut e = if i == 0 {
                    widgets::text(format!("{bump}"))
                } else { Element::column(Vec::new()) };
                e.style.width = Dim::px(400.0);
                e.style.height = Dim::px(16.0);
                e
            }).collect::<Vec<_>>())
        }, secs),
    }
}
