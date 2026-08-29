use lumen_core::geometry::Size;
use lumen_widgets::{widgets, App, Element};
fn main() {
    let mut h = App::new(|cx| {
        let mut f: Element = lumen_widgets::TextField::new(cx, "f", "type here")
            .id("field")
            .into();
        f.style.flex_grow = 1.0;
        let mut col: Element = widgets::column(vec![f, widgets::text("below").id("below")]);
        col.style.width = lumen_layout::Dim::px(200.0);
        col
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    for id in ["field", "below"] {
        match h.node_bounds_by_id(id) {
            Some(b) => println!(
                "{id}: x{:.0} y{:.0} w{:.0} h{:.0}",
                b.x0,
                b.y0,
                b.width(),
                b.height()
            ),
            None => println!("{id}: MISSING"),
        }
    }
}
