//! Renders the accordion FAQ to a PNG (and is the binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = accordion::main_app().run_headless(Size::new(520.0, 620.0));
    let stats = a.pump();
    std::fs::write("/tmp/accordion.png", a.screenshot().to_png()).unwrap();
    println!(
        "accordion: {} nodes -> /tmp/accordion.png",
        stats.node_count
    );
}
