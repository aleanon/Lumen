//! Renders svg to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = svg::main_app().run_headless(Size::new(520.0, 420.0));
    let s = a.pump();
    std::fs::write("/tmp/svg.png", a.screenshot().to_png()).unwrap();
    println!("svg: {} nodes -> /tmp/svg.png", s.node_count);
}
