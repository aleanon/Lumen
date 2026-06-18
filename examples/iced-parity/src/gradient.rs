//! gradient — a bold linear-gradient strip on a Canvas (E8.6), presented as a
//! centred hero panel like the rest of the gallery.
use kurbo::Rect;
use lumen_core::Color;
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

/// Build the gradient app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;
    theme::center_screen(theme::panel_centered(widgets::column(vec![
        theme::caption("LINEAR GRADIENT").id("title"),
        widgets::canvas(300.0, 140.0, |f, size| {
            f.linear_gradient_rect(
                Rect::new(0.0, 0.0, size.width, size.height),
                Color::srgb8(0x1a, 0x73, 0xe8, 0xff),
                Color::srgb8(0xe8, 0x1a, 0x4b, 0xff),
            );
        })
        .id("gradient"),
        theme::caption("blue to rose, interpolated in Oklab"),
    ])))
}
