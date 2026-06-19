//! Renders image to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = image::main_app().run_headless(Size::new(560.0, 540.0));
    let s = a.pump();
    std::fs::write("/tmp/image.png", a.screenshot().to_png()).unwrap();
    println!("image: {} nodes -> /tmp/image.png", s.node_count);
}
