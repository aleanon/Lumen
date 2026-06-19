//! Renders the counter to a PNG (and is the binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = counter::main_app().run_headless(Size::new(440.0, 520.0));
    let stats = a.pump();
    std::fs::write("/tmp/counter.png", a.screenshot().to_png()).unwrap();
    println!("counter: {} nodes -> /tmp/counter.png", stats.node_count);
}
