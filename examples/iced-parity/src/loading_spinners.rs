//! loading_spinners — an indeterminate spinner: a 3/4 arc whose rotation is a
//! pure function of `now_ms()` (E8.9/E8.11). `cx.animate()` keeps the host
//! producing frames; the shell advances the clock by real elapsed time, so it
//! spins on its own. Deterministic: advancing the clock rotates it by an exact
//! angle.
use kurbo::{Affine, BezPath, Point};
use lumen_core::Color;
use lumen_widgets::{theme, widgets, App, BuildCx, Element};
use std::f64::consts::PI;

/// Build the spinner app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    theme::screen("Loading", body(cx))
}

fn body(cx: &mut BuildCx) -> Element {
    cx.animate();
    // One revolution every 1.2 seconds.
    let angle = cx.now_ms() / 1000.0 * (2.0 * PI / 1.2);
    let spinner = widgets::canvas(64.0, 64.0, move |f, size| {
        let c = Point::new(size.width / 2.0, size.height / 2.0);
        // A 3/4 arc approximated by line segments, rotated by `angle`.
        let mut arc = BezPath::new();
        let r = 24.0;
        for i in 0..=27 {
            let a = i as f64 / 36.0 * 2.0 * PI;
            let p = Point::new(c.x + r * a.cos(), c.y + r * a.sin());
            if i == 0 {
                arc.move_to(p);
            } else {
                arc.line_to(p);
            }
        }
        f.with_transform(
            Affine::translate((c.x, c.y)) * Affine::rotate(angle) * Affine::translate((-c.x, -c.y)),
            |f| f.stroke(&arc, Color::srgb8(0x1a, 0x73, 0xe8, 0xff), 4.0),
        );
    })
    .id("spinner");
    widgets::column(vec![spinner, widgets::text("Loading…").id("label")])
}
