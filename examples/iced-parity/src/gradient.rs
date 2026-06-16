//! gradient — a linear gradient fill on a Canvas (E8.6).
use kurbo::Rect;
use lumen_core::Color;
use lumen_widgets::{widgets, App, BuildCx, Element};

/// Build the gradient app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;
    widgets::column(vec![
        widgets::text("Linear gradient").id("title"),
        widgets::canvas(220.0, 80.0, |f, size| {
            f.linear_gradient_rect(
                Rect::new(0.0, 0.0, size.width, size.height),
                Color::srgb8(0x1a, 0x73, 0xe8, 0xff),
                Color::srgb8(0xe8, 0x1a, 0x4b, 0xff),
            );
        })
        .id("gradient"),
    ])
}
