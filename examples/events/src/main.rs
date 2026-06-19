//! Renders events to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = events::main_app().run_headless(Size::new(460.0, 440.0));
    let s = a.pump();
    std::fs::write("/tmp/events.png", a.screenshot().to_png()).unwrap();
    println!("events: {} nodes -> /tmp/events.png", s.node_count);
}
