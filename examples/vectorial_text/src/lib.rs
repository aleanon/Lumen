//! vectorial_text — M.6: text as GEOMETRY. Glyph outlines come out of the
//! text stack (`TextBlock::outlines`, swash Béziers) and are drawn through
//! the immediate-mode Canvas: filled, stroked, and scaled — effects a glyph
//! atlas can't do.
use kurbo::Affine;
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use lumen_text::{TextEngine, TextStyle};
use lumen_widgets::{widgets, App, BuildCx, Element};

/// Build the vectorial-text app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;
    let mut canvas = widgets::canvas(460.0, 240.0, |frame, _size| {
        let mut eng = TextEngine::new();
        let block = eng.layout(
            "Lumen",
            TextStyle {
                font_size: 64.0,
                ..TextStyle::default()
            },
            &[],
            None,
            lumen_text::TextAlign::Start,
        );
        let outlines = block.outlines();
        // Filled, at the top.
        frame.with_transform(Affine::translate((30.0, 20.0)), |f| {
            for p in &outlines {
                f.fill(p, lumen_core::Color::srgb8(0x2b, 0x6c, 0xff, 0xff));
            }
        });
        // Stroked only — the outline itself.
        frame.with_transform(Affine::translate((30.0, 100.0)), |f| {
            for p in &outlines {
                f.stroke(p, lumen_core::Color::srgb8(0xf7, 0x4c, 0x00, 0xff), 1.5);
            }
        });
        // Scaled 1.8× from the same geometry — no re-raster, no blur.
        frame.with_transform(Affine::translate((30.0, 150.0)) * Affine::scale(1.8), |f| {
            for p in &outlines {
                f.fill(p, lumen_core::Color::srgb8(0x18, 0xc2, 0x7d, 0x66));
            }
        });
    });
    canvas = canvas.id("vector");

    let mut col = widgets::column(vec![
        widgets::text("Glyph outlines → Canvas").id("title"),
        canvas,
    ])
    .id("page");
    col.style = LayoutStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        width: Dim::pct(1.0),
        height: Dim::pct(1.0),
        align_items: Some(Align::Center),
        justify_content: Some(Align::Center),
        row_gap: Dim::px(10.0),
        ..LayoutStyle::default()
    };
    col
}
