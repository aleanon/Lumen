//! Bisect Lumen's per-row steady-state cost by REMOVING work from the view.
//!
//! No profiler on this box (perf_event_paranoid=4, no valgrind), so the method
//! is subtractive: build variants that each drop one candidate cost and see
//! what the frame time does. 3000 rows, one changing per frame, NullRenderer.
use kurbo::Size;
use lumen_core::state::Signal;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, BuildCx, Element};
use std::time::Instant;

const N: usize = 3000;
const ITERS: u32 = 60;

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
    for _ in 0..5 { sig.update(h.runtime(), |v| *v += 1); h.pump(); }
    let t0 = Instant::now();
    for _ in 0..ITERS { sig.update(h.runtime(), |v| *v += 1); h.pump(); }
    let us = t0.elapsed().as_secs_f64() * 1e6 / f64::from(ITERS);
    println!("{label:<44} {us:>9.1} us   {:>7.3} us/row", us / N as f64);
}

fn main() {
    // A. baseline: the benchmark's own view
    time("A text rows (benchmark baseline)", |cx| {
        let bump = cx.signal("n", || 0i64).get(cx.runtime());
        widgets::column((0..N).map(|i| if i == 0 {
            widgets::text(format!("counter: {bump}"))
        } else { widgets::text(format!("row {i}")) }).collect::<Vec<_>>())
    });

    // B. same shape, but every row carries an explicit size: text is still
    //    shaped for painting, but layout no longer has to MEASURE it.
    time("B text rows + explicit width/height", |cx| {
        let bump = cx.signal("n", || 0i64).get(cx.runtime());
        widgets::column((0..N).map(|i| {
            let mut e = if i == 0 {
                widgets::text(format!("counter: {bump}"))
            } else { widgets::text(format!("row {i}")) };
            e.style.width = Dim::px(400.0);
            e.style.height = Dim::px(16.0);
            e
        }).collect::<Vec<_>>())
    });

    // C. no text at all: empty fixed-size boxes. Isolates the per-node
    //    pipeline (element -> tree -> taffy -> paint) from anything textual.
    time("C empty fixed-size boxes (no text)", |cx| {
        let bump = cx.signal("n", || 0i64).get(cx.runtime());
        let _ = bump;
        widgets::column((0..N).map(|_| {
            let mut e = Element::column(Vec::new());
            e.style.width = Dim::px(400.0);
            e.style.height = Dim::px(16.0);
            e
        }).collect::<Vec<_>>())
    });

    // D. C, but the signal is read so the frame is genuinely dirty each time
    //    (guards against C being optimised into an idle pump).
    time("D empty boxes, one carries the signal", |cx| {
        let bump = cx.signal("n", || 0i64).get(cx.runtime());
        widgets::column((0..N).map(|i| {
            let mut e = if i == 0 {
                widgets::text(format!("{bump}"))
            } else {
                Element::column(Vec::new())
            };
            e.style.width = Dim::px(400.0);
            e.style.height = Dim::px(16.0);
            e
        }).collect::<Vec<_>>())
    });
}
