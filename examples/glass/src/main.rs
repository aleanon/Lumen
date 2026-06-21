//! Renders glass to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = glass::main_app().run_headless(Size::new(600.0, 520.0));
    let s = a.pump();
    std::fs::write("/tmp/glass.png", a.screenshot().to_png()).unwrap();
    println!("glass: {} nodes -> /tmp/glass.png", s.node_count);
}
