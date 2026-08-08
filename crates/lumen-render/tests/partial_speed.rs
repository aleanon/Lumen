//! What R6.2 + R6.3 actually buy, measured rather than assumed.
//!
//! Reported as a table, not gated: the CPU readback in `render_at_scale` is a
//! fixed per-frame cost the present path does not pay, so any ratio here
//! UNDERSTATES the live win. A budget written against it would be a budget
//! against readback.
#![cfg(feature = "wgpu")]

mod common;

use common::*;
use kurbo::Rect;
use lumen_core::Color;
use lumen_render::display_list::*;
use lumen_render::gpu::Wgpu;
use std::time::Instant;

const W: u32 = 1280;
const H: u32 = 800;

/// A list with `n` rects spread over the frame — the shape a long list makes.
fn scene(n: usize, moved: f64) -> DisplayList {
    let mut dl = DisplayList::new();
    for i in 0..n {
        let y = (i % 40) as f64 * 20.0;
        let x = (i / 40) as f64 * 24.0;
        dl.push(DrawCmd::Rect {
            rect: Rect::new(x, y, x + 20.0, y + 16.0),
            brush: Brush::Solid(Color::srgb8(0x30, 0x60, 0xb0, 0xff)),
            radii: CornerRadii::all(3.0),
            border: None,
        });
    }
    dl.push(DrawCmd::Rect {
        rect: Rect::new(600.0 + moved, 400.0, 640.0 + moved, 440.0),
        brush: Brush::Solid(Color::srgb8(0xd0, 0x20, 0x20, 0xff)),
        radii: CornerRadii::all(4.0),
        border: None,
    });
    dl
}

fn best(mut f: impl FnMut()) -> f64 {
    let mut b = f64::MAX;
    for _ in 0..7 {
        let t = Instant::now();
        f();
        b = b.min(t.elapsed().as_secs_f64() * 1e6);
    }
    b
}

#[test]
fn report_partial_redraw_cost() {
    let Some(gpu) = Wgpu::new() else {
        eprintln!("no adapter; skipping");
        return;
    };
    eprintln!("adapter: {}", gpu.adapter_name());
    // The readback floor: an empty scene still copies W*H*4 bytes back. The
    // present path never pays it, so it is subtracted below to expose the
    // encode+draw work — which is the only thing damage can remove.
    let empty = DisplayList::new();
    gpu.render_at_scale(&empty, W, H, 1.0, bg());
    let floor = best(|| {
        gpu.render_at_scale(&empty, W, H, 1.0, bg());
    });
    eprintln!("readback floor (empty scene): {floor:.1} µs");
    eprintln!(
        "{:>8}  {:>10}  {:>10}  {:>10}  {:>10}  {:>6}",
        "cmds", "full", "damaged", "full-draw", "dmg-draw", "ratio"
    );
    // One 40x40 element moved: the damage a scroll-free UI update makes.
    let dirty = Rect::new(598.0, 398.0, 654.0, 442.0);
    for &n in &[500usize, 2000, 8000, 20000] {
        let a = scene(n, 0.0);
        let b = scene(n, 12.0);
        gpu.render_at_scale(&a, W, H, 1.0, bg());
        let full = best(|| {
            gpu.render_at_scale(&b, W, H, 1.0, bg());
        });
        gpu.render_at_scale(&a, W, H, 1.0, bg());
        let part = best(|| {
            gpu.render_at_scale_dirty(&b, W, H, 1.0, bg(), Some(dirty));
        });
        let (fd, pd) = ((full - floor).max(0.1), (part - floor).max(0.1));
        eprintln!(
            "{n:>8}  {full:>10.1}  {part:>10.1}  {fd:>10.1}  {pd:>10.1}  {:>6.2}",
            pd / fd
        );
    }
}
