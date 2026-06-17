//! stopwatch — a real running timer: large bold centred readout with Start/Stop
//! and Reset on a row beneath, inside a rounded, soft-shadowed panel centred on
//! the screen. It runs off the virtual clock (E8.9): while running it
//! accumulates elapsed time each frame, so `just win stopwatch` ticks on its own
//! and a test drives it deterministically with `advance()`.
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

/// Build the stopwatch app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let elapsed = cx.signal("elapsed_ms", || 0.0f64);
    let running = cx.signal("running", || false);
    let last = cx.signal("last_ms", || 0.0f64);
    let rt = cx.runtime();

    // Accumulate wall/virtual time while running: add the delta since the last
    // frame, then remember this frame's clock. (Handlers can't read the clock,
    // so the running total is integrated here in the build.)
    let now = cx.now_ms();
    let on = running.get(rt);
    let prev = last.get(rt);
    if on {
        elapsed.update(rt, |e| *e += (now - prev).max(0.0));
        cx.animate(); // keep frames coming so it ticks live
    }
    last.set(rt, now);

    let total = elapsed.get(rt) as i64 / 1000;
    let readout = format!("{:02}:{:02}", total / 60, total % 60);

    let toggle = theme::accent_button(if on { "Stop" } else { "Start" }, move |rt| {
        running.update(rt, |r| *r = !*r)
    })
    .id("toggle");
    let reset = theme::ghost_button("Reset", move |rt| {
        elapsed.set(rt, 0.0);
        running.set(rt, false);
    })
    .id("reset");

    theme::center_screen(theme::panel_centered(widgets::column(vec![
        theme::display(readout).id("display"),
        theme::button_row(vec![toggle, reset]),
    ])))
}
