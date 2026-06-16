//! stopwatch — a running timer (start/stop, tick).
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

/// Build the stopwatch app.
pub fn main_app() -> App {
    App::new(build)
}
fn build(cx: &mut BuildCx) -> Element {
    theme::screen("Stopwatch", body(cx))
}

fn body(cx: &mut BuildCx) -> Element {
    let elapsed = cx.signal("elapsed", || 0i64);
    let running = cx.signal("running", || false);
    let e = elapsed.get(cx.runtime());
    let on = running.get(cx.runtime());
    widgets::column(vec![
        widgets::text(format!("{:02}:{:02}", e / 60, e % 60)).id("display"),
        widgets::button(if on { "Stop" } else { "Start" }, move |rt| {
            running.update(rt, |r| *r = !*r)
        })
        .id("toggle"),
        // A tick advances time only while running (the shell's frame clock does
        // this automatically; exposed as a button for deterministic tests).
        widgets::button("tick", move |rt| {
            if running.get(rt) {
                elapsed.update(rt, |x| *x += 1)
            }
        })
        .id("tick"),
        widgets::button("Reset", move |rt| elapsed.set(rt, 0)).id("reset"),
    ])
}
