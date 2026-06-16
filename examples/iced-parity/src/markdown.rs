//! markdown — render a Markdown document (headings, lists, emphasis, code).
use lumen_widgets::{markdown, theme, widgets, App, BuildCx, Element};

const DOC: &str = "# Lumen\n\nA Rust GUI framework with *first-class* agents.\n\n## Features\n\n- deterministic rendering\n- `.lss` styling\n- hot reload\n";

/// Build the markdown app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    theme::screen("Markdown", body(cx))
}

fn body(cx: &mut BuildCx) -> Element {
    let _ = cx;
    widgets::column(vec![markdown::render(DOC)]).id("root")
}
