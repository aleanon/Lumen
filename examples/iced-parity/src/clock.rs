//! clock — an analog clock face drawn on a Canvas (E8.1). The time is a signal
//! the agent can advance (deterministic, no wall clock).
use kurbo::{Affine, BezPath, Point};
use lumen_core::Color;
use lumen_widgets::{widgets, App, BuildCx, Element};
use std::f64::consts::PI;

/// Build the clock app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let seconds = cx.signal("seconds", || 0i64);
    let s = seconds.get(cx.runtime());
    let face = widgets::canvas(120.0, 120.0, move |f, size| {
        let c = Point::new(size.width / 2.0, size.height / 2.0);
        f.fill_circle(c, 56.0, Color::srgb8(0x20, 0x24, 0x28, 0xff));
        f.fill_circle(c, 52.0, Color::WHITE);
        // Second hand: 6° per second.
        let angle = (s % 60) as f64 * (PI / 30.0);
        let mut hand = BezPath::new();
        hand.move_to(c);
        hand.line_to((c.x, c.y - 46.0));
        f.with_transform(
            Affine::translate((c.x, c.y)) * Affine::rotate(angle) * Affine::translate((-c.x, -c.y)),
            |f| f.stroke(&hand, Color::srgb8(0xe8, 0x1a, 0x4b, 0xff), 2.0),
        );
        f.fill_circle(c, 4.0, Color::BLACK);
    })
    .id("face");
    widgets::column(vec![
        face,
        widgets::button("tick", move |rt| seconds.update(rt, |x| *x += 1)).id("tick"),
        widgets::text(format!("{:02}:{:02}", s / 60 % 60, s % 60)).id("digital"),
    ])
}
