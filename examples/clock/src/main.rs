//! Renders clock to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = clock::main_app().run_headless(Size::new(460.0, 500.0));
    let s = a.pump();
    std::fs::write("/tmp/clock.png", a.screenshot().to_png()).unwrap();
    println!("clock: {} nodes -> /tmp/clock.png", s.node_count);
}
