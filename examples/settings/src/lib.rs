//! M1-exit: the "settings app" — 3 screens (Tabs), themed and styleable from
//! `.lss`, with a text input, hot-reloadable, and drivable by `lumen-agent`.

use lumen_widgets::{widgets, widgets_m1, App, BuildCx, Element};

/// The default stylesheet (hot-reloadable at runtime). Tokens drive the theme.
pub const STYLESHEET: &str = r#"
@tokens { accent: #1a73e8ff; }
@theme light { panel: #ffffffff; }
@theme dark  { panel: #101418ff; }
#title { color: $accent; }
"#;

/// Build the settings UI (the app's root closure, reused by the mobile shells).
pub fn build(cx: &mut BuildCx) -> Element {
    let tab = cx.signal("tab", || 0usize);
    let current = tab.get(cx.runtime());

    let header = widgets::row(vec![
        widgets::text("Settings").id("title"),
        widgets_m1::spacer(),
    ]);
    let nav = widgets_m1::tabs(cx, "tab", &["General", "Appearance", "About"]);

    let screen = match current {
        0 => widgets::column(vec![
            widgets_m1::switch(cx, "notifications", "Notifications").id("notifications"),
            widgets_m1::stepper(cx, "volume", 0, 10).id("volume"),
        ]),
        1 => widgets::column(vec![
            widgets_m1::switch(cx, "dark_mode", "Dark mode").id("dark_mode"),
            widgets::text("Accent color").id("accent_label"),
        ]),
        _ => widgets::column(vec![
            widgets::text("Lumen Settings 1.0").id("about"),
            widgets::text_field_basic(cx, "username", "").id("username"),
        ]),
    };

    widgets::column(vec![header, nav, widgets_m1::divider(), screen]).id("screen")
}

/// Build the settings application.
pub fn main_app() -> App {
    App::new(build).stylesheet(STYLESHEET)
}
