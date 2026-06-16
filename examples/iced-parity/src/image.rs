//! image — display a decoded image (a generated test pattern stands in for a
//! loaded asset).
use lumen_render::media::{TestPattern, VideoSource};
use lumen_widgets::{widgets, App, BuildCx, Element};

/// Build the image app.
pub fn main_app() -> App {
    App::new(build)
}
fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;
    let img = TestPattern.frame_at(0.5, 96, 64);
    widgets::column(vec![
        widgets::text("Image viewer").id("title"),
        widgets::image(img).id("photo"),
    ])
}
