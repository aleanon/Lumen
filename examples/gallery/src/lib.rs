//! A Storybook-class **component gallery** (T7.2): built-in widgets alongside a
//! third-party one (`widget_rating::rating`), all self-tested through the agent.

use lumen_widgets::{widgets, App, BuildCx, Element};

/// Build the gallery.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    widgets::column(vec![
        widgets::text("Component gallery").id("title"),
        widgets::button("Button", |_| {}).id("button"),
        widgets::switch(cx, "wifi", "Wi-Fi").id("switch"),
        widgets::select(cx, "pick", &["A", "B", "C"]).id("select"),
        // A third-party widget, driven exactly like the built-ins.
        widget_rating::rating(cx, "stars", 5),
    ])
}
