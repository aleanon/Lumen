//! toast — show a transient notification; it auto-dismisses after a few ticks.
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

/// Build the toast app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    // `ttl` > 0 means a toast is showing; ticking decrements it to 0.
    let ttl = cx.signal("ttl", || 0i64);
    let remaining = ttl.get(cx.runtime());

    let mut col = vec![theme::button_row(vec![
        theme::accent_button("Notify", move |rt| ttl.set(rt, 3)).id("notify"),
        theme::ghost_button("tick", move |rt| ttl.update(rt, |t| *t = (*t - 1).max(0))).id("tick"),
    ])];
    if remaining > 0 {
        col.push(theme::badge("✓ Saved", theme::success()).id("toast"));
    }

    theme::center_screen(theme::panel_centered(widgets::column(col)))
}
