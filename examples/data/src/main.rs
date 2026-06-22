//! Renders data to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = data::main_app().run_headless(Size::new(460.0, 460.0));
    // Two pumps settle the inline-executor resource (dispatch, then drain).
    a.pump();
    let s = a.pump();
    std::fs::write("/tmp/data.png", a.screenshot().to_png()).unwrap();
    println!("data: {} nodes -> /tmp/data.png", s.node_count);
}
