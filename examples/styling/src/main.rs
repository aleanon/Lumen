//! Renders the styling showcase to a PNG (and is the binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = styling::main_app().run_headless(Size::new(760.0, 760.0));
    let stats = a.pump();
    std::fs::write("/tmp/styling.png", a.screenshot().to_png()).unwrap();
    println!("styling: {} nodes -> /tmp/styling.png", stats.node_count);
}
