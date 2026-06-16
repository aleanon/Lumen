//! progress_bar — a determinate bar driven by +/- buttons.
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

/// Build the progress-bar app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    theme::screen("Progress", body(cx))
}

fn body(cx: &mut BuildCx) -> Element {
    let pct = cx.signal("pct", || 30i64);
    let p = pct.get(cx.runtime());
    widgets::column(vec![
        widgets::progress_bar(p as f64 / 100.0).id("bar"),
        widgets::row(vec![
            widgets::button("-10", move |rt| pct.update(rt, |x| *x = (*x - 10).max(0))).id("less"),
            widgets::text(format!("{p}%")).id("label"),
            widgets::button("+10", move |rt| pct.update(rt, |x| *x = (*x + 10).min(100)))
                .id("more"),
        ]),
    ])
}
