//! Renders widget_gallery to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = widget_gallery::main_app().run_headless(Size::new(620.0, 980.0));
    let s = a.pump();
    std::fs::write("/tmp/widget_gallery.png", a.screenshot().to_png()).unwrap();
    println!(
        "widget_gallery: {} nodes -> /tmp/widget_gallery.png",
        s.node_count
    );
}
