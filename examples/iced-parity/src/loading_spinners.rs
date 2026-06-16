//! loading_spinners — an indeterminate spinner: an arc rotated by an angle the
//! agent (or the animation clock) advances.
use kurbo::{Affine, BezPath, Point};
use lumen_core::Color;
use lumen_widgets::{widgets, App, BuildCx, Element};
use std::f64::consts::PI;

/// Build the spinner app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let step = cx.signal("step", || 0i64);
    let s = step.get(cx.runtime());
    let spinner = widgets::canvas(64.0, 64.0, move |f, size| {
        let c = Point::new(size.width / 2.0, size.height / 2.0);
        // A 3/4 arc approximated by line segments, rotated by the step.
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
        let angle = s as f64 * (PI / 8.0);
        f.with_transform(
            Affine::translate((c.x, c.y)) * Affine::rotate(angle) * Affine::translate((-c.x, -c.y)),
            |f| f.stroke(&arc, Color::srgb8(0x1a, 0x73, 0xe8, 0xff), 4.0),
        );
    })
    .id("spinner");
    widgets::column(vec![
        spinner,
        widgets::button("advance", move |rt| step.update(rt, |x| *x += 1)).id("advance"),
    ])
}
