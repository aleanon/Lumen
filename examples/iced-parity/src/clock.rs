//! clock — an analog clock face drawn on a Canvas (E8.1), animated off the
//! virtual clock (E8.9/E8.11). The hands are a pure function of `now_ms()`, and
//! `cx.animate()` asks the host to keep frames coming; on the desktop the shell
//! advances the clock by real elapsed time. Deterministic: a test advances the
//! clock and the hands land at an exact position.
use kurbo::{Affine, BezPath, Point};
use lumen_core::Color;
use lumen_widgets::{widgets, App, BuildCx, Element};
use std::f64::consts::PI;

/// Build the clock app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    cx.animate();
    let ink = Color::srgb8(0x20, 0x24, 0x28, 0xff);
    let accent = Color::srgb8(0xe8, 0x1a, 0x4b, 0xff);
    let total = cx.now_ms() / 1000.0; // seconds since start
    let secs = total % 60.0;
    let mins = (total / 60.0) % 60.0;
    let face = widgets::canvas(140.0, 140.0, move |f, size| {
        let c = Point::new(size.width / 2.0, size.height / 2.0);
        f.fill_circle(c, 64.0, ink);
        f.fill_circle(c, 60.0, Color::WHITE);
        // A hand: a line from the centre, rotated `angle` about the centre.
        let mut draw_hand = |angle: f64, len: f64, width: f64, color: Color| {
            let mut p = BezPath::new();
            p.move_to(c);
            p.line_to((c.x, c.y - len));
            f.with_transform(
                Affine::translate((c.x, c.y))
                    * Affine::rotate(angle)
                    * Affine::translate((-c.x, -c.y)),
                |f| f.stroke(&p, color, width),
            );
        };
        draw_hand(mins * (PI / 30.0), 40.0, 3.0, ink); // minute
        draw_hand(secs * (PI / 30.0), 52.0, 2.0, accent); // second
        f.fill_circle(c, 4.0, ink);
    })
    .id("face");
    let whole = total as i64;
    widgets::column(vec![
        face,
        widgets::text(format!("{:02}:{:02}", whole / 60 % 60, whole % 60)).id("digital"),
    ])
}
