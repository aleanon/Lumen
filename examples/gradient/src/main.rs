//! Renders gradient to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = gradient::main_app().run_headless(Size::new(520.0, 480.0));
    let s = a.pump();
    std::fs::write("/tmp/gradient.png", a.screenshot().to_png()).unwrap();
    println!("gradient: {} nodes -> /tmp/gradient.png", s.node_count);
}
