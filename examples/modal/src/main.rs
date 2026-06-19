//! Renders modal to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = modal::main_app().run_headless(Size::new(540.0, 460.0));
    let s = a.pump();
    std::fs::write("/tmp/modal.png", a.screenshot().to_png()).unwrap();
    println!("modal: {} nodes -> /tmp/modal.png", s.node_count);
}
