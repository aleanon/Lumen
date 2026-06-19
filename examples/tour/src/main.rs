//! Renders tour to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = tour::main_app().run_headless(Size::new(540.0, 460.0));
    let s = a.pump();
    std::fs::write("/tmp/tour.png", a.screenshot().to_png()).unwrap();
    println!("tour: {} nodes -> /tmp/tour.png", s.node_count);
}
