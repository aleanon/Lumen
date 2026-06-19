//! Renders toast to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = toast::main_app().run_headless(Size::new(520.0, 460.0));
    let s = a.pump();
    std::fs::write("/tmp/toast.png", a.screenshot().to_png()).unwrap();
    println!("toast: {} nodes -> /tmp/toast.png", s.node_count);
}
