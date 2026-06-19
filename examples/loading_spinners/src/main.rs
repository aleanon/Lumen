//! Renders loading_spinners to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = loading_spinners::main_app().run_headless(Size::new(520.0, 360.0));
    let s = a.pump();
    std::fs::write("/tmp/loading_spinners.png", a.screenshot().to_png()).unwrap();
    println!(
        "loading_spinners: {} nodes -> /tmp/loading_spinners.png",
        s.node_count
    );
}
