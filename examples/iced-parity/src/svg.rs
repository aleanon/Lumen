//! svg — render an SVG asset to an image.
use lumen_widgets::{widgets, App, BuildCx, Element};

const ICON: &str = "<svg width=\"64\" height=\"64\"><rect x=\"8\" y=\"8\" width=\"48\" height=\"48\" fill=\"#2ea043\"/><circle cx=\"32\" cy=\"32\" r=\"14\" fill=\"#ffffff\"/></svg>";

/// Build the svg app.
pub fn main_app() -> App {
    App::new(build)
}
fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;
    let img = lumen_render::svg::render(ICON, 64, 64, lumen_core::Color::WHITE);
    widgets::column(vec![
        widgets::text("SVG asset").id("title"),
        widgets::image(img).id("icon"),
    ])
}
