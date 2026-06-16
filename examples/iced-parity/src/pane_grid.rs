//! pane_grid — two resizable panes; drag to move the split (E8.4).
use lumen_widgets::widgets_extra::pane_grid;
use lumen_widgets::{widgets, App, BuildCx, Element};

/// Build the pane-grid app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let left = widgets::text("Left pane").id("left");
    let right = widgets::text("Right pane").id("right");
    pane_grid(cx, "split", left, right)
}
