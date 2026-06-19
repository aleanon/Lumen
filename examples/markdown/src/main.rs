//! Renders markdown to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = markdown::main_app().run_headless(Size::new(560.0, 560.0));
    let s = a.pump();
    std::fs::write("/tmp/markdown.png", a.screenshot().to_png()).unwrap();
    println!("markdown: {} nodes -> /tmp/markdown.png", s.node_count);
}
