//! changelog — a longer Markdown document inside a scroll container.
use lumen_widgets::{markdown, widgets, App, BuildCx, Element};

const LOG: &str = "# Changelog\n\n## 2.0\n\n- agent auto-repair loop\n- web + mobile parity\n\n## 1.0\n\n- the `.lss` styling system\n- the test + agent harness\n";

/// Build the changelog app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    widgets::scroll(cx, "log", 200.0, 600.0, vec![markdown::render(LOG)]).id("root")
}
