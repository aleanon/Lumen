//! Bench target host. See `benches/vs_egui.rs` and `benches/vs_iced.rs`.
//!
//! The tests below are SANITY CHECKS on the harness, not on the frameworks.
//! A competitive benchmark whose opponent silently does nothing produces a
//! flattering number and no warning, which is how the egui comparison was
//! wrong for a day (it charged Lumen for a rasterizer egui never ran). These
//! assert that each side actually performs the work being timed.

#[cfg(test)]
mod masonry_harness_is_honest {
    use masonry::core::{NewWidget, Widget, WidgetTag};
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::{Flex, Label};

    fn harness(n: usize) -> TestHarness<Flex> {
        let tag: WidgetTag<Label> = WidgetTag::new("counter");
        let mut flex = Flex::column()
            .with_child(NewWidget::new_with_tag(Label::new("counter: 0"), tag));
        for i in 1..n {
            flex = flex.with_child(Label::new(format!("row {i}")).with_auto_id());
        }
        TestHarness::create_with_size(
            default_property_set(),
            flex.with_auto_id(),
            masonry::kurbo::Size::new(400.0, 800.0),
        )
    }

    /// SKIP_RENDER_TESTS must actually short-circuit the GPU submission.
    ///
    /// `render()` builds a vello Scene and updates the AccessKit tree, then
    /// rasterizes through vello on a real device. The benchmark's whole
    /// stopping-point claim rests on that last step being skipped; if the env
    /// var stopped being honoured, the masonry side would silently start
    /// including a GPU round trip that Lumen's NullRenderer never does.
    #[test]
    fn skip_render_tests_stops_before_the_gpu() {
        // SAFETY: this test process is single-threaded at this point.
        unsafe { std::env::set_var("SKIP_RENDER_TESTS", "1") };
        let mut h = harness(10);
        let img = h.render();
        assert_eq!(
            (img.width(), img.height()),
            (1, 1),
            "with SKIP_RENDER_TESTS set, render() must return the 1x1 \
             placeholder — a real frame here means the benchmark is timing a \
             GPU rasterization the Lumen side never performs"
        );
    }

    /// …and masonry must really lay out every row, as Lumen does.
    #[test]
    fn masonry_lays_out_every_row() {
        unsafe { std::env::set_var("SKIP_RENDER_TESTS", "1") };
        // Both sizes must OVERFLOW the 800px window: a Flex column fills the
        // viewport, so at small N the root rect is clamped to 800 and says
        // nothing about how many rows were laid out. (This assertion caught
        // that on its first run — with 2 vs 40 rows it read 800 vs 1110 and
        // looked like culling, which it was not.)
        let mut a = harness(200);
        let _ = a.render();
        let h200 = a.root_widget().ctx().local_layout_rect().height();

        let mut b = harness(400);
        let _ = b.render();
        let h400 = b.root_widget().ctx().local_layout_rect().height();

        assert!(h200 > 800.0, "200 rows must overflow the window, got {h200}");
        assert!(
            h400 > h200 * 1.8,
            "doubling the rows must roughly double the laid-out height \
             ({h400} vs {h200}); if it does not, masonry is culling and Lumen \
             — which does not — would be doing strictly more work"
        );
    }
}

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
