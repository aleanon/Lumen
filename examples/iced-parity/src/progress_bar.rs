//! progress_bar — a determinate bar with a big percentage readout and −/+
//! controls, centred in a soft-shadowed panel.
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

/// Build the progress-bar app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let pct = cx.signal("pct", || 30i64);
    let p = pct.get(cx.runtime());
    theme::center_screen(theme::panel_centered(widgets::column(vec![
        theme::display(format!("{p}%")).id("label"),
        theme::fixed_width(widgets::progress_bar(p as f64 / 100.0), 280.0).id("bar"),
        theme::button_row(vec![
            theme::ghost_button("−10", move |rt| pct.update(rt, |x| *x = (*x - 10).max(0)))
                .id("less"),
            theme::accent_button("+10", move |rt| pct.update(rt, |x| *x = (*x + 10).min(100)))
                .id("more"),
        ]),
    ])))
}
