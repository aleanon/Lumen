//! sierpinski — a recursively-subdivided triangle on a Canvas. `depth` is a
//! signal driven by +/- buttons.
use kurbo::{BezPath, Point};
use lumen_core::Color;
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

/// Build the Sierpinski app.
pub fn main_app() -> App {
    App::new(build)
}

fn tri(f: &mut lumen_render::canvas::Frame, a: Point, b: Point, c: Point, depth: u32) {
    if depth == 0 {
        let mut p = BezPath::new();
        p.move_to(a);
        p.line_to(b);
        p.line_to(c);
        p.close_path();
        f.fill(&p, Color::srgb8(0x1a, 0x73, 0xe8, 0xff));
        return;
    }
    let mid = |x: Point, y: Point| Point::new((x.x + y.x) / 2.0, (x.y + y.y) / 2.0);
    tri(f, a, mid(a, b), mid(a, c), depth - 1);
    tri(f, mid(a, b), b, mid(b, c), depth - 1);
    tri(f, mid(a, c), mid(b, c), c, depth - 1);
}

fn build(cx: &mut BuildCx) -> Element {
    let depth = cx.signal("depth", || 4u32);
    let d = depth.get(cx.runtime());
    let canvas = widgets::canvas(200.0, 180.0, move |f, size| {
        tri(
            f,
            Point::new(size.width / 2.0, 4.0),
            Point::new(4.0, size.height - 4.0),
            Point::new(size.width - 4.0, size.height - 4.0),
            d,
        );
    })
    .id("fractal");
    theme::center_screen(theme::panel_centered(widgets::column(vec![
        canvas,
        theme::button_row(vec![
            theme::ghost_button("–", move |rt| {
                depth.update(rt, |x| *x = x.saturating_sub(1))
            })
            .id("less"),
            theme::fixed_width(theme::caption(format!("depth {d}")), 70.0).id("depth"),
            theme::accent_button("+", move |rt| depth.update(rt, |x| *x = (*x + 1).min(7)))
                .id("more"),
        ]),
    ])))
}
