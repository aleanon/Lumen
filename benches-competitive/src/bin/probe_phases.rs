//! Subtractive bisection of Lumen's per-row steady-state cost.
//!
//! Each variant runs in its OWN process invocation (`probe_phases A|B|C|D`).
//! Run sequentially in one process they shared allocator state — the first
//! variant warmed the pages the next one measured — and the first version of
//! this probe reported a +/-40% spread even pinned to a P-core. 400 timed
//! iterations after 100 warm-up, median reported, p10/p90 shown so the spread
//! is visible rather than assumed. Pin it: `taskset -c 2 ...` (this box is a
//! hybrid i9-13900KF; migrating between P- and E-cores moves the number more
//! than anything being measured).
use kurbo::Size;
use lumen_core::state::Signal;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, BuildCx, Element};
use std::time::Instant;

const N: usize = 3000;
const ITERS: usize = 400;

struct NullRenderer;
impl lumen_render::Renderer for NullRenderer {
    fn render_frame(&mut self, _l: &lumen_render::DisplayList, _w: u32, _h: u32, _s: f64,
                    _b: lumen_core::Color) -> lumen_render::RgbaImage {
        lumen_render::RgbaImage::from_raw(0, 0, Vec::new())
    }
    fn name(&self) -> &'static str { "null" }
}

fn time(label: &str, build: impl Fn(&mut BuildCx) -> Element + 'static) {
    let mut h = App::new(build)
        .with_renderer(NullRenderer)
        .run_headless(Size::new(400.0, 800.0));
    h.pump();
    let sig: Signal<i64> = h.runtime().signal("n", || 0);
    for _ in 0..100 { sig.update(h.runtime(), |v| *v += 1); h.pump(); }
    let mut s = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        sig.update(h.runtime(), |v| *v += 1);
        let t = Instant::now();
        h.pump();
        s.push(t.elapsed().as_secs_f64() * 1e6);
    }
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = s[s.len() / 2];
    println!(
        "{label:<40} {med:>8.1} us  {:>6.3} us/row   (p10 {:.0} p90 {:.0})",
        med / N as f64, s[s.len() / 10], s[s.len() * 9 / 10]
    );
}

fn main() {
    match std::env::args().nth(1).unwrap_or_else(|| "A".into()).as_str() {
        // A. the benchmark's own view
        "A" => time("A text rows (baseline)", |cx| {
            let bump = cx.signal("n", || 0i64).get(cx.runtime());
            widgets::column((0..N).map(|i| if i == 0 {
                widgets::text(format!("counter: {bump}"))
            } else { widgets::text(format!("row {i}")) }).collect::<Vec<_>>())
        }),
        // B. text still shaped for painting, but layout need not MEASURE it
        "B" => time("B text rows + explicit size", |cx| {
            let bump = cx.signal("n", || 0i64).get(cx.runtime());
            widgets::column((0..N).map(|i| {
                let mut e = if i == 0 {
                    widgets::text(format!("counter: {bump}"))
                } else { widgets::text(format!("row {i}")) };
                e.style.width = Dim::px(400.0);
                e.style.height = Dim::px(16.0);
                e
            }).collect::<Vec<_>>())
        }),
        // C. no text at all: the per-node pipeline floor
        _ => time("C empty boxes, no text", |cx| {
            let bump = cx.signal("n", || 0i64).get(cx.runtime());
            widgets::column((0..N).map(|i| {
                let mut e = if i == 0 {
                    widgets::text(format!("{bump}"))
                } else { Element::column(Vec::new()) };
                e.style.width = Dim::px(400.0);
                e.style.height = Dim::px(16.0);
                e
            }).collect::<Vec<_>>())
        }),
    }
}
