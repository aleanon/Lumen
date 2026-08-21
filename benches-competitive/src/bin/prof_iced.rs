//! A steady-state iced frame loop for `perf record` — the mirror of
//! `prof_target memo`, so the two profiles can be compared directly.
//!
//! Same view as `benches/vs_iced.rs`: 3000 text rows in a column, row 0's
//! content changing every frame, a real `iced_tiny_skia` renderer (NOT
//! `iced_core`'s `impl Renderer for ()`, whose `type Paragraph = ()` makes
//! shaping a no-op and the comparison meaningless).
use iced_core::{
    layout::{self, Limits},
    mouse, renderer as core_renderer,
    widget::Tree,
    Element, Font, Length, Pixels, Rectangle, Theme,
};
use std::time::{Duration, Instant};

const N: usize = 3000;
const VIEW_W: f32 = 400.0;
const VIEW_H: f32 = 800.0;

type IcedElement<'a> = Element<'a, (), Theme, iced_tiny_skia::Renderer>;

fn iced_view<'a>(n: usize, bump: i64) -> IcedElement<'a> {
    let rows: Vec<IcedElement<'a>> = (0..n)
        .map(|i| {
            let s = if i == 0 { format!("counter: {bump}") } else { format!("row {i}") };
            iced_widget::text(s).into()
        })
        .collect();
    iced_widget::Column::with_children(rows).width(Length::Fixed(VIEW_W)).into()
}

fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(12);
    let mut renderer = iced_tiny_skia::Renderer::new(Font::default(), Pixels(16.0));
    let theme = Theme::Light;
    let style = core_renderer::Style::default();
    let viewport = Rectangle::new([0.0, 0.0].into(), [VIEW_W, VIEW_H].into());
    let limits = Limits::new([0.0, 0.0].into(), [VIEW_W, VIEW_H].into());

    let first = iced_view(N, 0);
    let mut tree = Tree::new(&first);
    let mut bump = 0i64;

    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut frames = 0u64;
    while Instant::now() < deadline {
        bump += 1;
        let mut view = iced_view(N, bump);
        tree.diff(&view);
        let node = view.as_widget_mut().layout(&mut tree, &renderer, &limits);
        view.as_widget().draw(
            &tree, &mut renderer, &theme, &style,
            layout::Layout::new(&node), mouse::Cursor::Unavailable, &viewport,
        );
        frames += 1;
    }
    eprintln!("{frames} frames");
}
