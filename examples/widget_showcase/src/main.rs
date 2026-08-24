//! Renders widget_showcase to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = widget_showcase::main_app().run_headless(Size::new(980.0, 760.0));
    let s = a.pump();
    std::fs::write("/tmp/widget_showcase.png", a.screenshot().to_png()).unwrap();
    println!(
        "widget_showcase: {} nodes -> /tmp/widget_showcase.png",
        s.node_count
    );
}
