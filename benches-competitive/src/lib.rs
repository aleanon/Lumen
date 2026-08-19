//! Bench target host. See `benches/vs_egui.rs` and `benches/vs_iced.rs`.
//!
//! The tests below are SANITY CHECKS on the harness, not on the frameworks.
//! A competitive benchmark whose opponent silently does nothing produces a
//! flattering number and no warning, which is how the egui comparison was
//! wrong for a day (it charged Lumen for a rasterizer egui never ran). These
//! assert that each side actually performs the work being timed.

#[cfg(test)]
mod harness_is_honest {
    use iced_core::{
        layout::Limits, widget::Tree, Element, Font, Length, Pixels, Size, Theme, Widget,
    };

    type El<'a> = Element<'a, (), Theme, iced_tiny_skia::Renderer>;

    fn view<'a>(n: usize) -> El<'a> {
        let rows: Vec<El<'a>> = (0..n)
            .map(|i| iced_widget::text(format!("row {i}")).into())
            .collect();
        iced_widget::Column::with_children(rows)
            .width(Length::Fixed(400.0))
            .into()
    }

    /// iced must actually SHAPE the text, not stub it.
    ///
    /// `iced_core`'s built-in null renderer is `impl Renderer for ()` with
    /// `type Paragraph = ()`, so with it every row measures 0 high and the
    /// column collapses. `iced_tiny_skia` has a real cosmic-text paragraph.
    /// If this ever reports a zero height, the benchmark is comparing Lumen's
    /// text stack against nothing.
    #[test]
    fn iced_lays_out_real_text() {
        let renderer = iced_tiny_skia::Renderer::new(Font::default(), Pixels(16.0));
        let limits = Limits::new(Size::ZERO, Size::new(400.0, 800.0));

        let mut one = view(1);
        let mut tree = Tree::new(&one);
        let h1 = one
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .size()
            .height;

        let mut ten = view(10);
        let mut tree10 = Tree::new(&ten);
        let h10 = ten
            .as_widget_mut()
            .layout(&mut tree10, &renderer, &limits)
            .size()
            .height;

        assert!(h1 > 0.0, "a shaped row must have height, got {h1}");
        assert!(
            h10 > h1 * 5.0,
            "10 rows must be much taller than 1 ({h10} vs {h1}); if these are \
             equal or zero, text shaping is a no-op and the comparison is void"
        );
    }

    /// …and the row count must reach the layout tree, so both sides really do
    /// N rows of work rather than collapsing to a constant.
    #[test]
    fn iced_layout_has_one_child_per_row() {
        let renderer = iced_tiny_skia::Renderer::new(Font::default(), Pixels(16.0));
        let limits = Limits::new(Size::ZERO, Size::new(400.0, 800.0));
        let mut v = view(250);
        let mut tree = Tree::new(&v);
        let node = v.as_widget_mut().layout(&mut tree, &renderer, &limits);
        assert_eq!(
            node.children().len(),
            250,
            "every row must be laid out; iced does not cull here, and neither \
             does Lumen, so both pay the full N"
        );
    }
}
