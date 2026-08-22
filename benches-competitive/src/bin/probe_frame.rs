//! One frame-cost method for every framework in the comparison.
//!
//! BENCH3 mixed criterion means (Rust) with hand-rolled best-of-N (GTK), which
//! makes the rows hard to compare and sensitive to whatever else the machine is
//! doing. Everything here — and the Qt/GTK harnesses beside it — reports the
//! MINIMUM of N iterations after a warm-up. The minimum is the least-interfered
//! sample, which is what makes a number survive a noisy box; this matters
//! because a background process at 100% CPU has twice during this work made a
//! measurement read as a large regression that did not exist.
//!
//! Same content everywhere: a 3000-row list, one row's text changing per frame.
use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{bind, widgets, App};
use std::time::Instant;

use iced_core::{
    layout::{self, Limits},
    mouse, renderer as core_renderer,
    widget::Tree,
    Element as IcedElement, Font, Length, Pixels, Rectangle, Theme,
};

const VIEW_W: f32 = 400.0;
const VIEW_H: f32 = 800.0;

struct NullRenderer;
impl lumen_render::Renderer for NullRenderer {
    fn render_frame(&mut self, _l: &lumen_render::DisplayList, _w: u32, _h: u32, _s: f64,
                    _b: lumen_core::Color) -> lumen_render::RgbaImage {
        lumen_render::RgbaImage::from_raw(0, 0, Vec::new())
    }
    fn name(&self) -> &'static str { "null" }
}

fn report(label: &str, us: f64) { println!("{label:<30} {us:>9.1} us"); }

fn main() {
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(3000);
    let iters: u32 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(200);

    // ---- Lumen, rebuild path: the signal is read in the view, so the change
    //      is structural and the frame is rebuilt (memoized per row).
    {
        let mut h = App::new(move |cx| {
            let bump = cx.signal("n", || 0i64).get(cx.runtime());
            let rows: Vec<_> = (0..n).map(|i| {
                let dep = if i == 0 { bump } else { 0 };
                cx.scope_with_deps(("row", i), dep, move |_cx| {
                    if i == 0 { widgets::text(format!("counter: {bump:05}")) }
                    else { widgets::text(format!("row {i}")) }
                })
            }).collect();
            widgets::column(rows)
        }).with_renderer(NullRenderer).run_headless(Size::new(VIEW_W as f64, VIEW_H as f64));
        h.pump();
        let sig: Signal<i64> = h.runtime().signal("n", || 0);
        for _ in 0..20 { sig.update(h.runtime(), |v| *v += 1); h.pump(); }
        let mut best = f64::MAX;
        for _ in 0..iters {
            sig.update(h.runtime(), |v| *v += 1);
            let t = Instant::now();
            std::hint::black_box(h.pump());
            best = best.min(t.elapsed().as_secs_f64() * 1e6);
        }
        report("lumen/rebuild", best);
    }

    // ---- Lumen, patch path: the text is a binding, so the same change patches.
    {
        let mut h = App::new(move |cx| {
            let _ = cx.signal("n", || 0i64);
            let rows: Vec<_> = (0..n).map(|i| {
                let t = widgets::text(format!("row {i}"));
                if i == 0 {
                    t.bind_text(bind!(rt => {
                        let s: Signal<i64> = rt.signal("n", || 0i64);
                        format!("counter: {:05}", s.get(rt) % 100000)
                    }))
                } else { t }
            }).collect();
            widgets::column(rows)
        }).with_renderer(NullRenderer).run_headless(Size::new(VIEW_W as f64, VIEW_H as f64));
        h.pump();
        let sig: Signal<i64> = h.runtime().signal("n", || 0);
        for _ in 0..20 { sig.update(h.runtime(), |v| *v += 1); h.pump(); }
        let mut best = f64::MAX;
        for _ in 0..iters {
            sig.update(h.runtime(), |v| *v += 1);
            let t = Instant::now();
            std::hint::black_box(h.pump());
            best = best.min(t.elapsed().as_secs_f64() * 1e6);
        }
        report("lumen/patch (bound text)", best);
    }

    // ---- iced: diff + layout + draw, real tiny-skia renderer (a REAL
    //      Paragraph, so text is genuinely shaped — iced_core's null renderer
    //      would make shaping a no-op).
    {
        type El<'a> = IcedElement<'a, (), Theme, iced_tiny_skia::Renderer>;
        fn view<'a>(n: usize, bump: i64) -> El<'a> {
            let rows: Vec<El<'a>> = (0..n).map(|i| {
                let s = if i == 0 { format!("counter: {bump:05}") } else { format!("row {i}") };
                iced_widget::text(s).into()
            }).collect();
            iced_widget::Column::with_children(rows).width(Length::Fixed(VIEW_W)).into()
        }
        let mut renderer = iced_tiny_skia::Renderer::new(Font::default(), Pixels(16.0));
        let theme = Theme::Light;
        let style = core_renderer::Style::default();
        let viewport = Rectangle::new([0.0, 0.0].into(), [VIEW_W, VIEW_H].into());
        let limits = Limits::new([0.0, 0.0].into(), [VIEW_W, VIEW_H].into());
        let first = view(n, 0);
        let mut tree = Tree::new(&first);
        let mut bump = 0i64;
        let mut run = |tree: &mut Tree, bump: &mut i64| {
            *bump += 1;
            let mut v = view(n, *bump);
            tree.diff(&v);
            let node = v.as_widget_mut().layout(tree, &renderer, &limits);
            v.as_widget().draw(tree, &mut renderer, &theme, &style,
                layout::Layout::new(&node), mouse::Cursor::Unavailable, &viewport);
        };
        for _ in 0..20 { run(&mut tree, &mut bump); }
        let mut best = f64::MAX;
        for _ in 0..iters {
            let t = Instant::now();
            run(&mut tree, &mut bump);
            best = best.min(t.elapsed().as_secs_f64() * 1e6);
        }
        report("iced/frame", best);
    }
    // ---- masonry (Xilem's widget + layout layer) via its own TestHarness.
    //      SKIP_RENDER_TESTS makes render() stop after building the vello
    //      Scene — the matched stopping point (Lumen's display list, iced's
    //      primitives), rather than rasterising on the GPU.
    {
        use masonry::core::{NewWidget, Widget, WidgetTag};
        use masonry::testing::TestHarness;
        use masonry::theme::default_property_set;
        use masonry::widgets::{Flex, Label};
        std::env::set_var("SKIP_RENDER_TESTS", "1");
        let tag: WidgetTag<Label> = WidgetTag::new("counter");
        let mut flex = Flex::column()
            .with_child(NewWidget::new_with_tag(Label::new("counter: 00000"), tag));
        for i in 1..n {
            flex = flex.with_child(Label::new(format!("row {i}")).with_auto_id());
        }
        let mut h = TestHarness::create_with_size(
            default_property_set(),
            flex.with_auto_id(),
            masonry::kurbo::Size::new(VIEW_W as f64, VIEW_H as f64),
        );
        let _ = h.render();
        let mut bump = 0i64;
        macro_rules! step {
            () => {{
                bump += 1;
                let b = bump;
                h.edit_widget(tag, |mut l| Label::set_text(&mut l, format!("counter: {b:05}")));
                let _ = h.render();
            }};
        }
        for _ in 0..20 { step!(); }
        let mut best = f64::MAX;
        for _ in 0..iters {
            let t = Instant::now();
            step!();
            best = best.min(t.elapsed().as_secs_f64() * 1e6);
        }
        report("masonry/frame (scene)", best);
    }

    // ---- egui: immediate mode. `ctx.run` rebuilds the whole UI, then
    //      `tessellate` turns shapes into meshes — its display-list analogue.
    {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(VIEW_W, VIEW_H),
            )),
            ..Default::default()
        };
        input.time = Some(0.0);
        let mut counter = 0i64;
        let mut run = |counter: &mut i64| {
            *counter += 1;
            let c = *counter;
            let out = ctx.run(input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    for i in 0..n {
                        if i == 0 { ui.label(format!("counter: {c:05}")); }
                        else { ui.label(format!("row {i}")); }
                    }
                });
            });
            std::hint::black_box(ctx.tessellate(out.shapes, out.pixels_per_point));
        };
        for _ in 0..20 { run(&mut counter); }
        let mut best = f64::MAX;
        for _ in 0..iters {
            let t = Instant::now();
            run(&mut counter);
            best = best.min(t.elapsed().as_secs_f64() * 1e6);
        }
        report("egui/frame (tessellated)", best);
    }

    println!("({n} rows, best of {iters})");
}
