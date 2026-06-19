//! Renders sierpinski to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = sierpinski::main_app().run_headless(Size::new(460.0, 560.0));
    let s = a.pump();
    std::fs::write("/tmp/sierpinski.png", a.screenshot().to_png()).unwrap();
    println!("sierpinski: {} nodes -> /tmp/sierpinski.png", s.node_count);
}
