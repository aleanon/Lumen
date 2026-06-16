//! styling — theme a widget tree from `.lss`.
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

const LSS: &str = "@tokens { accent: #1a73e8ff; }\n#title { color: $accent; }\n";

/// Build the styling app.
pub fn main_app() -> App {
    App::new(build).stylesheet(LSS)
}
fn build(cx: &mut BuildCx) -> Element {
    theme::screen("Styling", body(cx))
}

fn body(cx: &mut BuildCx) -> Element {
    let _ = cx;
    widgets::column(vec![
        widgets::text("Styled title").id("title"),
        widgets::button("A button", |_| {}).id("button"),
    ])
}
