//! F2 sizing: what does a pump cost when nothing changed?
//!
//! Splice-in-place leaves untouched subtrees alone entirely, so the idle pump
//! is the floor it approaches for the unchanged part of a tree. The gap
//! between it and a one-row-changed pump is the prize A.3.3 is chasing.
use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App};
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

fn med(mut v: Vec<f64>) -> f64 { v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[v.len() / 2] }

fn main() {
    let mut h = App::new(move |cx| {
        let bump = cx.signal("n", || 0i64).get(cx.runtime());
        widgets::column((0..N).map(|i| if i == 0 {
            widgets::text(format!("counter: {bump}"))
        } else { widgets::text(format!("row {i}")) }).collect::<Vec<_>>())
    })
    .with_renderer(NullRenderer)
    .run_headless(Size::new(400.0, 800.0));
    h.pump();
    let sig: Signal<i64> = h.runtime().signal("n", || 0);
    for _ in 0..50 { sig.update(h.runtime(), |v| *v += 1); h.pump(); }

    // one row changes
    let mut a = Vec::new();
    for _ in 0..200 {
        sig.update(h.runtime(), |v| *v += 1);
        let t = Instant::now(); let st = h.pump();
        a.push(t.elapsed().as_secs_f64() * 1e6);
        std::hint::black_box(st);
    }
    // nothing changes
    let mut b = Vec::new();
    for _ in 0..200 {
        let t = Instant::now(); let st = h.pump();
        b.push(t.elapsed().as_secs_f64() * 1e6);
        std::hint::black_box(st);
    }
    let (one, idle) = (med(a), med(b));
    println!("pump, ONE of {N} rows changed   {one:>9.1} us");
    println!("pump, NOTHING changed          {idle:>9.1} us");
    println!("=> the shallow walk costs      {:>9.1} us  ({:.0}x the idle pump)",
             one - idle, one / idle.max(0.001));
}
