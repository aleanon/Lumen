//! Renders changelog to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = changelog::main_app().run_headless(Size::new(560.0, 560.0));
    let s = a.pump();
    std::fs::write("/tmp/changelog.png", a.screenshot().to_png()).unwrap();
    println!("changelog: {} nodes -> /tmp/changelog.png", s.node_count);
}
