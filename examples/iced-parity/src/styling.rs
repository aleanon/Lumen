//! styling — theme a widget tree from `.lss` (the `#title` colour comes from a
//! token), shown as a centred hero panel.
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

const LSS: &str = "@tokens { accent: #1a73e8ff; }\n#title { color: $accent; }\n";

/// Build the styling app.
pub fn main_app() -> App {
    App::new(build).stylesheet(LSS)
}

fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;
    theme::center_screen(theme::panel_centered(widgets::column(vec![
        theme::caption("THEMED FROM .LSS"),
        widgets::text("Styled title").id("title"),
        theme::accent_button("A button", |_| {}).id("button"),
    ])))
}
